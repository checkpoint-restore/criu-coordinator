use std::{
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};

use criu_coordinator::constants::*;


#[derive(Clone, Copy)]
struct Step {
    id:     &'static str,
    deps:   &'static str,
    action: &'static str,
    expect: &'static str, // Expected keyword in server output
}

// Represents a scenario with a name and the sequence of client requests steps to execute.
struct Scenario {
    name:  &'static str,
    steps: Vec<Step>,
}

fn pick_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
}

fn spawn_server(port: u16) -> Child {
    Command::new("target/debug/criu-coordinator")
        .args(["server", "--address", "127.0.0.1", "--port", &port.to_string(), "--max-retries", "5"])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn server")
}

fn spawn_client(step: Step, port: u16) -> Child {
    Command::new("target/debug/criu-coordinator")
        .args([
            "client",
            "--id", step.id,
            "--deps", step.deps,
            "--action", step.action,
            "--images-dir", ".",
            "--port", &port.to_string(),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn client")
}

fn server_ready(addr: &str, retries: u32) -> bool {
    for _ in 0..retries {
        if TcpStream::connect(addr).is_ok() {
            return true;
        }
        thread::sleep(Duration::from_millis(100));
    }
    false
}

fn assert_step(child: Child, step: Step, scenario: &str) {
    let out = child.wait_with_output().expect("wait client");
    let combined = String::from_utf8_lossy(&out.stdout).to_string()
        + &String::from_utf8_lossy(&out.stderr);
    assert!(
        combined.contains(step.expect),
        "Scenario '{scenario}': step {{id: {}, action: {}}} expected '{}' but output was:\n{}",
        step.id,
        step.action,
        step.expect,
        combined
    );
}


fn run_test(s: Scenario) {
    println!("\n================  {}  =================\n", s.name);

    let port = pick_port();
    let addr = format!("127.0.0.1:{port}");
    let mut server = spawn_server(port);
    assert!(server_ready(&addr, 20), "server failed to start");

    // Group by ID to keep pre and post phases together
    use std::collections::HashMap;
    let mut phases: HashMap<&str, Vec<Step>> = HashMap::new();
    for step in &s.steps {
        phases.entry(step.id).or_default().push(*step);
    }

    let mut handles = vec![];
    for (_id, steps) in phases {
        let scenario = s.name;
        handles.push(thread::spawn(move || {
            for (i, step) in steps.iter().enumerate() {
                // Wait a moment between dump and restore phases to allow the clients of the dump phase to finish
                if step.action == ACTION_PRE_RESTORE && i > 0 && steps[i - 1].action == ACTION_POST_DUMP {
                    thread::sleep(Duration::from_secs(1));
                }

                let child = spawn_client(*step, port);
                assert_step(child, *step, scenario);
                thread::sleep(Duration::from_millis(80));
            }
        }));
    }

    for h in handles {
        h.join().expect("client thread join failed");
    }

    let _ = server.kill();
    let _ = server.wait();
    thread::sleep(Duration::from_millis(100));
}

#[test]
fn dump_single_client() {
    run_test(Scenario {
        name: "Dump single client",
        steps: vec![
            Step { id: "A", deps: "", action: ACTION_PRE_DUMP,  expect: MESSAGE_ACK },
            Step { id: "A", deps: "", action: ACTION_POST_DUMP, expect: MESSAGE_ACK },
        ],
    });
}

#[test]
fn restore_single_client() {
    run_test(Scenario {
        name: "Restore single client",
        steps: vec![
            Step { id: "A", deps: "", action: ACTION_PRE_RESTORE,  expect: MESSAGE_ACK },
        ],
    });
}


#[test]
fn dump_single_client_with_nonexistent_dep() {
    run_test(Scenario {
        name: "Dump single client with nonexistent dep",
        steps: vec![
            Step { id: "A", deps: "B", action: ACTION_PRE_DUMP,  expect: MESSAGE_TIMEOUT },
        ],
    });
}

#[test]
fn restore_single_client_with_nonexistent_dep() {
    run_test(Scenario {
        name: "Restore single client with nonexistent dep",
        steps: vec![
            Step { id: "A", deps: "B", action: ACTION_PRE_RESTORE,  expect: MESSAGE_TIMEOUT },
        ],
    });
}

#[test]
fn dump_two_interdependent_clients() {
    run_test(Scenario {
        name: "Dump two interdependent clients",
        steps: vec![
            // pre-dump phase
            Step { id: "A", deps: "B", action: ACTION_PRE_DUMP,  expect: MESSAGE_ACK },
            Step { id: "B", deps: "A", action: ACTION_PRE_DUMP,  expect: MESSAGE_ACK },
            // post-dump phase (after checkpoints written)
            Step { id: "A", deps: "B", action: ACTION_POST_DUMP, expect: MESSAGE_ACK },
            Step { id: "B", deps: "A", action: ACTION_POST_DUMP, expect: MESSAGE_ACK },
        ],
    });
}

#[test]
fn restore_two_interdependent_clients() {
    run_test(Scenario {
        name: "Restore two interdependent clients",
        steps: vec![
            Step { id: "A", deps: "B", action: ACTION_PRE_RESTORE,  expect: MESSAGE_ACK },
            Step { id: "B", deps: "A", action: ACTION_PRE_RESTORE,  expect: MESSAGE_ACK },
        ],
    });
}


#[test]
fn dump_three_interdependent_clients() {
    run_test(Scenario {
        name: "Dump three interdependent clients",
        steps: vec![
            // pre-dump phase
            Step { id: "A", deps: "B:C", action: ACTION_PRE_DUMP,  expect: MESSAGE_ACK },
            Step { id: "B", deps: "A:C", action: ACTION_PRE_DUMP,  expect: MESSAGE_ACK },
            Step { id: "C", deps: "A:B", action: ACTION_PRE_DUMP,  expect: MESSAGE_ACK },
            // post-dump phase (after checkpoints written)
            Step { id: "A", deps: "B:C", action: ACTION_POST_DUMP, expect: MESSAGE_ACK },
            Step { id: "B", deps: "A:C", action: ACTION_POST_DUMP, expect: MESSAGE_ACK },
            Step { id: "C", deps: "A:B", action: ACTION_POST_DUMP, expect: MESSAGE_ACK },
        ],
    });
}

#[test]
fn restore_three_interdependent_clients() {
    run_test(Scenario {
        name: "Restore three interdependent clients",
        steps: vec![
            Step { id: "A", deps: "B:C", action: ACTION_PRE_RESTORE,  expect: MESSAGE_ACK },
            Step { id: "B", deps: "A:C", action: ACTION_PRE_RESTORE,  expect: MESSAGE_ACK },
            Step { id: "C", deps: "A:B", action: ACTION_PRE_RESTORE,  expect: MESSAGE_ACK },
        ],
    });
}

#[test]
fn dump_and_restore_three_interdependent_clients() {
    run_test(Scenario {
        name: "Dump and restore three interdependent clients",
        steps: vec![
            // Pre-dump phase
            Step { id: "A", deps: "B:C", action: ACTION_PRE_DUMP,  expect: MESSAGE_ACK },
            Step { id: "B", deps: "A:C", action: ACTION_PRE_DUMP,  expect: MESSAGE_ACK },
            Step { id: "C", deps: "A:B", action: ACTION_PRE_DUMP,  expect: MESSAGE_ACK },
            // Post-dump phase (after checkpoints written)
            Step { id: "A", deps: "B:C", action: ACTION_POST_DUMP, expect: MESSAGE_ACK },
            Step { id: "B", deps: "A:C", action: ACTION_POST_DUMP, expect: MESSAGE_ACK },
            Step { id: "C", deps: "A:B", action: ACTION_POST_DUMP, expect: MESSAGE_ACK },
            // Pre-restore phase
            Step { id: "A", deps: "B:C", action: ACTION_PRE_RESTORE,  expect: MESSAGE_ACK },
            Step { id: "B", deps: "A:C", action: ACTION_PRE_RESTORE,  expect: MESSAGE_ACK },
            Step { id: "C", deps: "A:B", action: ACTION_PRE_RESTORE,  expect: MESSAGE_ACK },
        ],
    });
}
