use regex::Regex;
use rexpect::process;
use rexpect::spawn;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::Command;
use std::time;
use tokio::prelude::*;
use tokio::sync::watch;
use tokio::task;
use tokio::time::delay_for;

// hooks layout-change window-add, session-create

#[derive(Debug, Clone)]
struct Prompt {
    name: String,
    regex: Regex,
}

#[derive(Debug, Clone)]
struct Window {
    id: u32,
    panes: Vec<u32>,
    pane_names: HashMap<u32, String>,
}

#[derive(Debug, Clone)]
struct Tree {
    prompts: Vec<Prompt>,
    sessions: Vec<u32>,
    windows: Vec<Window>,
}

impl Tree {
    fn new(prompts: Vec<Prompt>) -> Self {
        let mut tree = Self {
            prompts: prompts,
            sessions: vec![],
            windows: vec![],
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
                    tree.push_window(window_id);
                    tree.add_pane_to_window(window_id, pane_id);
                }
                None => continue,
            }
        }
        tree.sessions.dedup();

        tree
    }
    fn refresh(&mut self) {
        self.sessions = vec![];
        self.windows = vec![];
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
                    self.push_window(window_id);
                    self.add_pane_to_window(window_id, pane_id);
                }
                None => continue,
            }
        }
        self.sessions.dedup();
    }
    fn push_window(&mut self, window_id: u32) {
        for window in &self.windows {
            if window.id == window_id {
                return;
            }
        }
        let window = Window::new(window_id);
        self.windows.push(window);
    }
    fn add_pane_to_window(&mut self, window_id: u32, pane_id: u32) {
        for window in &mut self.windows {
            if window.id == window_id {
                window.panes.push(pane_id);
                return;
            }
        }
        panic!("No window {} for pane {}", window_id, pane_id);
    }
    fn window_from_pane(self, pane_id: u32) -> u32 {
        for window in &self.windows {
            for pane in &window.panes {
                if pane == &pane_id {
                    return window.id;
                }
            }
        }
        panic!("Pane id {} does not belong to any windows", pane_id);
    }
    fn process_line(&self, line: &str) {
        for prompt in &self.prompts {
            match prompt.regex.captures(line) {
                Some(cap) => {
                    let pane_id = cap.get(1).unwrap().as_str().parse::<u32>().unwrap();

                    let title = match prompt.name.as_str() {
                        "titan" => {
                            let filler = cap.get(2).unwrap().as_str();
                            format!("{fill}@titan", fill = filler)
                        }
                        _ => format!("{}", prompt.name),
                    };
                    let window_id = self.clone().window_from_pane(pane_id);
                    /*
                    match Command::new("/usr/bin/tmux")
                    .arg("rename-window")
                    .arg("-t")
                    .arg(display)
                    .arg(title)
                    .output()
                    {
                    Ok(_) => (),
                    Err(e) => panic!(e),
                    }
                    */
                }
                None => (),
            }
        }
    }
}

impl Window {
    fn new(window_id: u32) -> Self {
        return Window {
            id: window_id,
            panes: vec![],
            pane_names: HashMap::new(),
        };
    }
}

// TODO tmux hook to start this upon session
// TODO get target session via cli arg and attach to that, this way one instance will run per
// session
// TODO add other hosts from config file
// TODO exit/sleep once all tmux sessions terminate

enum Instruction {
    Refresh,       // e.g. layout change
    AttachTo(u32), // attach to session id u32
    DoNothing,
}

#[tokio::main]
async fn main() {
    // first get running sessions, windows and panes of tmux
    // tmux list-panes -a -F #{session_id}#{window_id}#{pane_id}
    //let re_sess = Regex::new(r"^(\d+):\s+.*").unwrap();

    let prompts = vec![Prompt {
        name: "titan".to_string(),
        regex: Regex::new(r"^%output\s+(%\d+)\s+([A-Za-z\d]+)@titan:~\$\s*$").unwrap(),
    }];

    // 0. gather tree and spawn attaches to existing sessions
    // 1. start listening for changes
    // 2. change tree based on changes
    let mut tree = Tree::new(prompts);
    let (tx, rx) = watch::channel(tree.clone());

    for session_id in tree.sessions.clone() {
        let my_rx = rx.clone();
        task::spawn(async move {
            // attach to session command mode

            let my_session_id = session_id.clone();
            let cmd = format!("/usr/bin/tmux -C attach -t {}", my_session_id);
            let mut sess = spawn(&cmd, Some(0)).unwrap();
            let delay = time::Duration::from_millis(10);

            while sess.process.status().unwrap() == process::wait::WaitStatus::StillAlive {
                let r_line = sess.read_line();
                match r_line {
                    Ok(line) => {
                        let current_tree = my_rx.borrow();
                        current_tree.process_line(&line);
                    }
                    Err(_) => {
                        delay_for(delay).await;
                    }
                };
            }
        });
    }

    let listener = bind("/tmp/tmux_renamer.sock");
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => match process_input_from_tmux(stream) {
                Instruction::Refresh => {
                    println!("refresh tree");
                    tree.refresh();
                    tx.broadcast(tree.clone()).unwrap();
                }
                Instruction::AttachTo(session_id) => {
                    println!("supposed to attach to window {}", session_id);
                    let my_rx = rx.clone();
                    task::spawn(async move {
                        // attach to session command mode

                        let my_session_id = session_id.clone();
                        let cmd = format!("/usr/bin/tmux -C attach -t {}", my_session_id);
                        let mut sess = spawn(&cmd, Some(0)).unwrap();
                        let delay = time::Duration::from_millis(10);

                        while sess.process.status().unwrap()
                            == process::wait::WaitStatus::StillAlive
                        {
                            let r_line = sess.read_line();
                            match r_line {
                                Ok(line) => {
                                    let current_tree = my_rx.borrow();
                                    current_tree.process_line(&line);
                                }
                                Err(_) => {
                                    delay_for(delay).await;
                                }
                            };
                        }
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
    let re_layout_changed = Regex::new(r"^\s*layout\s+changed\s+").unwrap();
    if re_layout_changed.is_match(&input) {
        return Instruction::Refresh;
    }
    let re_sess_createad = Regex::new(r"^\s*session\s+created\s+\$(\d+)\s*$").unwrap();
    match re_sess_createad.captures(&input) {
        Some(session_id) => {
            let u32_sess = session_id.get(1).unwrap().as_str().parse::<u32>().unwrap();
            return Instruction::AttachTo(u32_sess);
        }
        None => (),
    }
    Instruction::DoNothing
}

fn bind(path: impl AsRef<Path>) -> std::os::unix::net::UnixListener {
    let path = path.as_ref();
    // https://stackoverflow.com/questions/40218416/how-do-i-close-a-unix-socket-in-rust
    std::fs::remove_file(path);
    std::os::unix::net::UnixListener::bind(path).unwrap()
}
