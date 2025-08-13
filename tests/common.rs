use std::{
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};

pub const CRIU_COORDINATOR_PATH: &str = "target/debug/criu-coordinator";

pub fn pick_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

pub fn server_ready(addr: &str, retries: u32) -> bool {
    for _ in 0..retries {
        if TcpStream::connect(addr).is_ok() {
            return true;
        }
        thread::sleep(Duration::from_millis(100));
    }
    false
}

pub fn spawn_server(port: u16) -> Child {
    Command::new(CRIU_COORDINATOR_PATH)
        .args([
            "server",
            "--address",
            "127.0.0.1",
            "--port",
            &port.to_string(),
            "--wait-timeout",
            "5",
        ])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("Failed to spawn criu-coordinator server. Did you run `cargo build`?")
}
