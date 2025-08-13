use std::{
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
    sync::{Arc, Barrier},
};

use criu_coordinator::constants::*;
mod common;
use common::*;


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
    stages: Vec<Vec<Step>>,
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

fn assert_step(child: Child, step: Step, scenario: &str) {
    let out = child.wait_with_output().expect("wait client");
    let combined = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
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

    thread::scope(|scope| {
        for (i, stage) in s.stages.iter().enumerate() {
            if stage.is_empty() {
                continue;
            }

            println!("\n--- Running Stage {}: Action '{}' ---\n", i + 1, stage[0].action);
            let num_clients = stage.len();
            let barrier = Arc::new(Barrier::new(num_clients));
            let mut handles = vec![];


            for step in stage {
                let barrier = Arc::clone(&barrier);
                let scenario_name = s.name;
                handles.push(scope.spawn(move || {
                    barrier.wait(); // Synchronize start of all clients for this action
                    let child = spawn_client(*step, port);
                    assert_step(child, *step, scenario_name);
                }));
            }

            for h in handles {
                h.join().expect("client thread join failed");
            }

            // Give the server a moment to process before the next action
            thread::sleep(Duration::from_millis(100));
        }
    });

    let _ = server.kill();
    let _ = server.wait();
    thread::sleep(Duration::from_millis(100));
}

#[test]
fn dump_single_client() {
    run_test(Scenario {
        name: "Checkpoint single client",
        stages: vec![
            vec![Step { id: "A", deps: "", action: ACTION_PRE_DUMP, expect: MESSAGE_ACK }],
            vec![Step { id: "A", deps: "", action: ACTION_POST_DUMP, expect: MESSAGE_ACK }],
        ],
    });
}

#[test]
fn restore_single_client() {
    run_test(Scenario {
        name: "Restore single client",
        stages: vec![
            vec![Step { id: "A", deps: "", action: ACTION_PRE_RESTORE,  expect: MESSAGE_ACK }],
        ],
    });
}


#[test]
fn dump_single_client_with_nonexistent_dep() {
    run_test(Scenario {
        name: "Checkpoint single client with nonexistent dep",
        stages: vec![
            vec![Step { id: "A", deps: "B", action: ACTION_PRE_DUMP,  expect: MESSAGE_TIMEOUT }],
        ],
    });
}

#[test]
fn restore_single_client_with_nonexistent_dep() {
    run_test(Scenario {
        name: "Restore single client with nonexistent dep",
        stages: vec![
            vec![Step { id: "A", deps: "B", action: ACTION_PRE_RESTORE,  expect: MESSAGE_TIMEOUT }],
        ],
    });
}

#[test]
fn dump_two_interdependent_clients() {
    run_test(Scenario {
        name: "Checkpoint two interdependent clients",
        stages: vec![
            vec![
                Step { id: "A", deps: "B", action: ACTION_PRE_DUMP, expect: MESSAGE_ACK },
                Step { id: "B", deps: "A", action: ACTION_PRE_DUMP, expect: MESSAGE_ACK },
            ],
            vec![
                Step { id: "A", deps: "B", action: ACTION_POST_DUMP, expect: MESSAGE_ACK },
                Step { id: "B", deps: "A", action: ACTION_POST_DUMP, expect: MESSAGE_ACK },
            ],
        ],
    });
}

#[test]
fn restore_two_interdependent_clients() {
    run_test(Scenario {
        name: "Restore two interdependent clients",
        stages: vec![
            vec![
                Step { id: "A", deps: "B", action: ACTION_PRE_RESTORE, expect: MESSAGE_ACK },
                Step { id: "B", deps: "A", action: ACTION_PRE_RESTORE, expect: MESSAGE_ACK },
            ],
        ],
    });
}


#[test]
fn dump_three_interdependent_clients() {
    run_test(Scenario {
        name: "Checkpoint three interdependent clients",
        stages: vec![
            vec![
                Step { id: "A", deps: "B:C", action: ACTION_PRE_DUMP, expect: MESSAGE_ACK },
                Step { id: "B", deps: "A:C", action: ACTION_PRE_DUMP, expect: MESSAGE_ACK },
                Step { id: "C", deps: "A:B", action: ACTION_PRE_DUMP, expect: MESSAGE_ACK },
            ],
            vec![
                Step { id: "A", deps: "B:C", action: ACTION_POST_DUMP, expect: MESSAGE_ACK },
                Step { id: "B", deps: "A:C", action: ACTION_POST_DUMP, expect: MESSAGE_ACK },
                Step { id: "C", deps: "A:B", action: ACTION_POST_DUMP, expect: MESSAGE_ACK },
            ],
        ],
    });
}

#[test]
fn restore_three_interdependent_clients() {
    run_test(Scenario {
        name: "Restore three interdependent clients",
        stages: vec![
            vec![
                Step { id: "A", deps: "B:C", action: ACTION_PRE_RESTORE, expect: MESSAGE_ACK },
                Step { id: "B", deps: "A:C", action: ACTION_PRE_RESTORE, expect: MESSAGE_ACK },
                Step { id: "C", deps: "A:B", action: ACTION_PRE_RESTORE, expect: MESSAGE_ACK },
            ],
        ],
    });
}

#[test]
fn dump_and_restore_three_interdependent_clients() {
    run_test(Scenario {
        name: "Checkpoint and restore three interdependent clients",
        stages: vec![
            vec![
                Step { id: "A", deps: "B:C", action: ACTION_PRE_DUMP, expect: MESSAGE_ACK },
                Step { id: "B", deps: "A:C", action: ACTION_PRE_DUMP, expect: MESSAGE_ACK },
                Step { id: "C", deps: "A:B", action: ACTION_PRE_DUMP, expect: MESSAGE_ACK },
            ],
            vec![
                Step { id: "A", deps: "B:C", action: ACTION_POST_DUMP, expect: MESSAGE_ACK },
                Step { id: "B", deps: "A:C", action: ACTION_POST_DUMP, expect: MESSAGE_ACK },
                Step { id: "C", deps: "A:B", action: ACTION_POST_DUMP, expect: MESSAGE_ACK },
            ],
            vec![
                Step { id: "A", deps: "B:C", action: ACTION_PRE_RESTORE, expect: MESSAGE_ACK },
                Step { id: "B", deps: "A:C", action: ACTION_PRE_RESTORE, expect: MESSAGE_ACK },
                Step { id: "C", deps: "A:B", action: ACTION_PRE_RESTORE, expect: MESSAGE_ACK },
            ],
        ],
    });
}

#[test]
fn dump_and_restore_client_server_with_network_hooks() {
    run_test(Scenario {
        name: "Checkpoint and restore a client-server with network hooks",
        stages: vec![
            // Checkpointing phase
            vec![
                Step { id: "B", deps: "", action: ACTION_PRE_DUMP, expect: MESSAGE_ACK },
                Step { id: "A", deps: "B", action: ACTION_PRE_DUMP, expect: MESSAGE_ACK },
            ],
            vec![
                Step { id: "B", deps: "", action: ACTION_NETWORK_LOCK, expect: MESSAGE_ACK },
                Step { id: "A", deps: "B", action: ACTION_NETWORK_LOCK, expect: MESSAGE_ACK },
            ],
            vec![
                Step { id: "B", deps: "", action: ACTION_POST_DUMP, expect: MESSAGE_ACK },
                Step { id: "A", deps: "B", action: ACTION_POST_DUMP, expect: MESSAGE_ACK },
            ],

            // Restore phase
            vec![
                Step { id: "B", deps: "", action: ACTION_PRE_RESTORE, expect: MESSAGE_ACK },
                Step { id: "A", deps: "B", action: ACTION_PRE_RESTORE, expect: MESSAGE_ACK },
            ],
            vec![
                Step { id: "B", deps: "", action: ACTION_POST_RESTORE, expect: MESSAGE_ACK },
                Step { id: "A", deps: "B", action: ACTION_POST_RESTORE, expect: MESSAGE_ACK },
            ],
            vec![
                Step { id: "B", deps: "", action: ACTION_NETWORK_UNLOCK, expect: MESSAGE_ACK },
                Step { id: "A", deps: "B", action: ACTION_NETWORK_UNLOCK, expect: MESSAGE_ACK },
            ],
            vec![
                Step { id: "B", deps: "", action: ACTION_POST_RESUME, expect: MESSAGE_ACK },
                Step { id: "A", deps: "B", action: ACTION_POST_RESUME, expect: MESSAGE_ACK },
            ],
        ],
    });
}
