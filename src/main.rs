use regex::Regex;
use rexpect::process;
use rexpect::spawn;
use std::process::Command;
use std::{thread, time};

// hooks layout-change window-add, session-create

#[derive(Debug)]
struct Prompt {
    name: String,
    regex: Regex,
}

#[derive(Debug)]
struct Window {
    id: u32,
    panes: Vec<u32>,
}

#[derive(Debug)]
struct Tree {
    sessions: Vec<u32>,
    windows: Vec<Window>,
}

impl Tree {
    fn new() -> Self {
        let mut tree = Self {
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
}

impl Window {
    fn new(window_id: u32) -> Self {
        return Window {
            id: window_id,
            panes: vec![],
        };
    }
}

// TODO tmux hook to start this upon session
// TODO get target session via cli arg and attach to that, this way one instance will run per
// session
// TODO add other hosts from config file
// TODO exit/sleep once all tmux sessions terminate

fn main() {
    // first get running sessions, windows and panes of tmux
    // tmux list-panes -a -F #{session_id}#{window_id}#{pane_id}
    //let re_sess = Regex::new(r"^(\d+):\s+.*").unwrap();

    let prompts = vec![Prompt {
        name: "titan".to_string(),
        regex: Regex::new(r"^%output\s+(%\d+)\s+([A-Za-z\d]+)@titan:~\$\s*$").unwrap(),
    }];

    let mut tree = Tree::new();
    panic!("asd");
    //let mut session = sessions[0];
    let session = "4";
    // attach to session command mode

    let delay = time::Duration::from_millis(10);

    let cmd = format!("/usr/bin/tmux -C attach -t {}", session);
    // find a way to set timeout for unlimited
    let mut sess = spawn(&cmd, Some(0)).unwrap();
    while sess.process.status().unwrap() == process::wait::WaitStatus::StillAlive {
        let r_line = sess.read_line();
        match r_line {
            Ok(x) => {
                process_line(&x, &prompts);
            }
            Err(_) => thread::sleep(delay),
        };
    }
}

fn process_line(line: &str, prompts: &Vec<Prompt>) {
    for prompt in prompts {
        match prompt.regex.captures(line) {
            Some(cap) => {
                let display = cap.get(1).unwrap().as_str();

                let title = match prompt.name.as_str() {
                    "titan" => {
                        let filler = cap.get(2).unwrap().as_str();
                        format!("{fill}@titan", fill = filler)
                    }
                    _ => format!("{}", prompt.name),
                };
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
            }
            None => (),
        }
    }
}
