use regex::Regex;
use rexpect::process;
use rexpect::spawn;
use std::process::Command;
use std::{thread, time};

#[derive(Debug)]
struct Prompt {
    name: String,
    regex: Regex,
}
// TODO paralell parsing for all sessions
// TODO detect new sessions as the launch
// TODO add other hosts from config file
// TODO exit/sleep once all tmux sessions terminate

fn main() {
    // first get running sessions of tmux
    let output = Command::new("/usr/bin/tmux").arg("ls").output().unwrap();
    let output_string = String::from_utf8(output.stdout).unwrap();
    let output_vec: Vec<&str> = output_string.split('\n').collect();

    let re_sess = Regex::new(r"^(\d+):\s+.*").unwrap();

    let mut sessions = Vec::new();
    let prompts = vec![Prompt {
        name: "titan".to_string(),
        regex: Regex::new(r"^%output\s+(%\d+)\s+([A-Za-z\d]+)@titan:~\$\s*$").unwrap(),
    }];

    for line in output_vec {
        match re_sess.captures(line) {
            Some(cap) => sessions.push(cap.get(1).unwrap().as_str()),
            None => continue,
        }
    }
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

    println!("{:?}", sessions);
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
