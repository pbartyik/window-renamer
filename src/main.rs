use regex::Regex;
use rexpect::process;
use rexpect::spawn;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::Command;
use std::time;
use tokio::prelude::*;
use tokio::sync::mpsc;
use tokio::task;
use tokio::time::delay_for;

#[derive(Debug, Clone)]
struct Prompt {
    name: String,
    regex: Regex,
}

#[derive(Debug, Clone)]
struct Tree {
    prompts: Vec<Prompt>,
    sessions: Vec<u32>,
    windows: HashMap<u32, Vec<u32>>, // Window ID, [Pane ID,]
    panes: HashMap<u32, String>,     // Pane ID, Pane title
}

impl Tree {
    fn new(prompts: Vec<Prompt>) -> Self {
        let mut tree = Self {
            prompts,
            sessions: vec![],
            windows: HashMap::new(),
            panes: HashMap::new(),
        };

        let output = Command::new("/usr/bin/tmux")
            .arg("list-panes")
            .arg("-a")
            .arg("-F")
            .arg("#{session_id}#{window_id}#{pane_id}")
            .output()
            .unwrap();
        let output_string = String::from_utf8(output.stdout).unwrap();
        let output_vec: Vec<&str> = output_string.split('\n').collect();

        let re_tmux = Regex::new(r"^\$(\d+)@(\d+)%(\d+)\s*$").unwrap();

        for line in output_vec {
            match re_tmux.captures(line) {
                Some(cap) => {
                    let session_id = cap.get(1).unwrap().as_str().parse::<u32>().unwrap();
                    let window_id = cap.get(2).unwrap().as_str().parse::<u32>().unwrap();
                    let pane_id = cap.get(3).unwrap().as_str().parse::<u32>().unwrap();

                    tree.sessions.push(session_id);
                    match tree.windows.get_mut(&window_id) {
                        Some(window) => {
                            window.push(pane_id);
                        }
                        None => {
                            tree.windows.insert(window_id, vec![pane_id]);
                        }
                    }
                    match tree.panes.get_mut(&pane_id) {
                        Some(_) => {}
                        None => {
                            tree.panes.insert(pane_id, String::new());
                        }
                    }
                }
                None => continue,
            }
        }
        tree.sessions.dedup();

        tree
    }
    fn refresh(&mut self) {
        println!("refresh called");
        self.sessions = vec![];
        self.windows = HashMap::new();
        let output = Command::new("/usr/bin/tmux")
            .arg("list-panes")
            .arg("-a")
            .arg("-F")
            .arg("#{session_id}#{window_id}#{pane_id}")
            .output()
            .unwrap();
        let output_string = String::from_utf8(output.stdout).unwrap();
        let output_vec: Vec<&str> = output_string.split('\n').collect();

        let re_tmux = Regex::new(r"^\$(\d+)@(\d+)%(\d+)\s*$").unwrap();

        for line in output_vec {
            match re_tmux.captures(line) {
                Some(cap) => {
                    let session_id = cap.get(1).unwrap().as_str().parse::<u32>().unwrap();
                    let window_id = cap.get(2).unwrap().as_str().parse::<u32>().unwrap();
                    let pane_id = cap.get(3).unwrap().as_str().parse::<u32>().unwrap();

                    self.sessions.push(session_id);
                    match self.windows.get_mut(&window_id) {
                        Some(window) => {
                            window.push(pane_id);
                        }
                        None => {
                            self.windows.insert(window_id, vec![pane_id]);
                        }
                    }
                    match self.panes.get_mut(&pane_id) {
                        Some(_) => {}
                        None => {
                            self.panes.insert(pane_id, String::new());
                        }
                    }
                }
                None => continue,
            }
        }
        self.sessions.dedup();
    }
    fn get_window_of_pane(self, pane_id: u32) -> Option<u32> {
        for (w_id, p_ids) in &self.windows {
            for p_id in p_ids {
                if p_id == &pane_id {
                    let window_id = *w_id;
                    return Some(window_id);
                }
            }
        }
        None
    }
    fn update_window_title(&self, window_id: u32) {
        let panes = self.windows.get(&window_id).unwrap();
        let mut pane_count = HashMap::new();
        let mut window_title = String::new();
        // process existing names and come up with concatenated name
        for pane in panes {
            let pane_name = self.panes.get(&pane).unwrap();
            match pane_count.get_mut(pane_name) {
                Some(count) => {
                    let new_count = *count + 1;
                    pane_count.insert(pane_name, new_count);
                }
                None => {
                    pane_count.insert(pane_name, 1);
                }
            }
        }
        for (pane_name, count) in &pane_count {
            match count {
                1 => {
                    window_title.push_str(&pane_name);
                }
                _ => {
                    let _title = format!("{} x {}", pane_name, count);
                    window_title.push_str(&_title);
                }
            }
        }
        let window_name = format!("@{}", window_id);
        match Command::new("/usr/bin/tmux")
            .arg("rename-window")
            .arg("-t")
            .arg(window_name)
            .arg(window_title)
            .output()
        {
            Ok(_) => (),
            Err(e) => panic!(e),
        }
    }
    fn update_pane_title(self, pane_id: u32) {
        let window_id = self.clone().get_window_of_pane(pane_id).unwrap();
        self.update_window_title(window_id.clone());
    }
    fn process_output(&mut self, line: &str) {
        for prompt in &self.prompts {
            match prompt.regex.captures(line) {
                Some(cap) => {
                    println!("cap '{:?}'", cap);
                    let pane_id = cap.get(1).unwrap().as_str().parse::<u32>().unwrap();

                    let title = match prompt.name.as_str() {
                        "titan" => {
                            let filler = cap.get(2).unwrap().as_str();
                            format!("{fill}@titan", fill = filler)
                        }
                        _ => format!("{}", prompt.name),
                    };
                    self.panes.insert(pane_id, title);
                    self.clone().update_pane_title(pane_id);
                }
                None => (),
            }
        }
    }
    fn remove_pane(&mut self, pane_id: u32) {
        self.panes.remove(&pane_id);
        let window_id = self.clone().get_window_of_pane(pane_id).unwrap();
        let index = self.windows[&window_id]
            .iter()
            .position(|pid| *pid == pane_id)
            .unwrap();
        let mut windows = self.windows.get_mut(&window_id).unwrap().clone();
        windows.remove(index);
        self.windows.insert(window_id, windows);
        //self.windows[&window_id].remove(index); // this produced error
        //self.windows[&window_id].remove_item(&pane_id); // this is unstable
        self.update_window_title(window_id);
    }
}

// TODO tmux hook to start this upon session
// TODO add other hosts from config file
// TODO exit/sleep once all tmux sessions terminate

enum TreeInstruction {
    ProcessLine(String),
    RemovePane(u32),
    Refresh,
}

enum Instruction {
    Refresh,         // e.g. layout change
    AttachTo(u32),   // attach to session id u32
    RemovePane(u32), // remove pane
    DoNothing,
}

#[tokio::main]
async fn main() {
    // first get running sessions, windows and panes of tmux
    // tmux list-panes -a -F #{session_id}#{window_id}#{pane_id}
    //let re_sess = Regex::new(r"^(\d+):\s+.*").unwrap();

    let prompts = vec![Prompt {
        name: "titan".to_string(),
        regex: Regex::new(r"^%output\s+%(\d+)\s+([A-Za-z\d]+)@titan:~\$\s*$").unwrap(),
    }];

    let mut tree = Tree::new(prompts);
    let (mut tx, mut rx) = mpsc::channel(100);

    for session_id in tree.sessions.clone() {
        let mut my_tx = tx.clone();
        let my_session_id = session_id.clone();

        task::spawn(async move {
            let delay = time::Duration::from_millis(10);
            let cmd = format!("/usr/bin/tmux -C attach -t {}", my_session_id);
            let mut sess = spawn(&cmd, Some(0)).unwrap();

            while sess.process.status().unwrap() == process::wait::WaitStatus::StillAlive {
                let r_line = sess.read_line();
                match r_line {
                    Ok(line) => {
                        my_tx.send(TreeInstruction::ProcessLine(line)).await;
                    }
                    Err(_) => {
                        delay_for(delay).await;
                    }
                };
            }
        });
    }
    task::spawn(async move {
        while let Some(msg) = rx.recv().await {
            match msg {
                TreeInstruction::ProcessLine(line) => {
                    tree.process_output(&line);
                }
                TreeInstruction::Refresh => {
                    tree.refresh();
                }
                TreeInstruction::RemovePane(pane_id) => {
                    tree.remove_pane(pane_id);
                }
            }
        }
    });

    let listener = bind("/tmp/tmux_renamer.sock");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => match process_input_from_tmux(stream) {
                Instruction::Refresh => {
                    println!("refresh tree");
                    tx.send(TreeInstruction::Refresh).await;
                }
                Instruction::RemovePane(pane_id) => {
                    tx.send(TreeInstruction::RemovePane(pane_id)).await;
                }
                Instruction::AttachTo(session_id) => {
                    let mut my_tx = tx.clone();
                    let my_session_id = session_id.clone();

                    task::spawn(async move {
                        let cmd = format!("/usr/bin/tmux -C attach -t {}", my_session_id);
                        let mut sess = spawn(&cmd, Some(0)).unwrap();
                        let delay = time::Duration::from_millis(10);

                        while sess.process.status().unwrap()
                            == process::wait::WaitStatus::StillAlive
                        {
                            let r_line = sess.read_line();
                            match r_line {
                                Ok(line) => {
                                    my_tx.send(TreeInstruction::ProcessLine(line)).await;
                                }
                                Err(_) => {
                                    delay_for(delay).await;
                                }
                            };
                        }
                        sess.exp_eof(); // read everything, so process doesn't hang
                    });
                }
                Instruction::DoNothing => (),
            },
            Err(e) => {
                panic!("something went wrong here {}", e);
            }
        }
    }
}

fn process_input_from_tmux(stream: std::os::unix::net::UnixStream) -> Instruction {
    // layout change should trigger refresh
    // session creation should trigger AttachTo
    let mut reader = BufReader::new(stream);
    let mut input = String::new();
    reader.read_line(&mut input).unwrap();
    println!("Got input: {:?}", input);
    let re_layout_changed = Regex::new(r"^\s*layout\s+changed\s*").unwrap();
    if re_layout_changed.is_match(&input) {
        return Instruction::Refresh;
    }
    let re_sess_createad = Regex::new(r"^\s*session\s+created\s+\$(\d+)\s*$").unwrap();
    match re_sess_createad.captures(&input) {
        Some(session_id) => {
            let u32_sess = session_id.get(1).unwrap().as_str().parse::<u32>().unwrap();
            return Instruction::AttachTo(u32_sess);
        }
        None => {}
    }
    let re_remove_pane = Regex::new(r"^\s*remove\s+pane\s+%?(\d+)\s*").unwrap();
    match re_remove_pane.captures(&input) {
        Some(pane_id) => {
            let u32_pane_id = pane_id.get(1).unwrap().as_str().parse::<u32>().unwrap();
            return Instruction::RemovePane(u32_pane_id);
        }
        None => {}
    }
    Instruction::DoNothing
}

fn bind(path: impl AsRef<Path>) -> std::os::unix::net::UnixListener {
    let path = path.as_ref();
    // https://stackoverflow.com/questions/40218416/how-do-i-close-a-unix-socket-in-rust
    std::fs::remove_file(path);
    std::os::unix::net::UnixListener::bind(path).unwrap()
}
