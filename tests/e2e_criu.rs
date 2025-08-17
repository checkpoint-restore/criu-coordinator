use std::{
    env,
    fs,
    io::Write,
    os::unix::process::CommandExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::Duration,
};
pub mod common;
use common::*;


// TestProcess hold information about the processes we are testing.
struct TestProcess {
    id: String,
    child: Option<Child>,
    pid: u32,
    image_dir: PathBuf,
    _dependencies: Vec<String>,
}

// TestGuard ensure cleanup is always called.
struct TestGuard {
    server: Child,
    processes: Vec<TestProcess>,
}

impl Drop for TestGuard {
    fn drop(&mut self) {
        cleanup(&mut self.server, &mut self.processes);
    }
}


pub fn get_pid_by_name(name: &str) -> Option<u32> {
    let output = Command::new("pidof").arg(name).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()?
        .split_whitespace()
        .next()?
        .parse::<u32>()
        .ok()
}

pub fn is_root() -> bool {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .expect("Failed to run `id -u` to check for root user.");
    String::from_utf8_lossy(&output.stdout).trim() == "0"
}

pub fn is_criu_installed() -> bool {
    Command::new("criu")
        .arg("--version")
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}


fn setup(port: u16) -> Vec<TestProcess> {
    println!("Setting up test environment");

    let make_status = Command::new("make")
        .current_dir("tests")
        .status()
        .expect("Failed to run `make` in tests directory");
    assert!(make_status.success(), "make command failed");

    let mut processes = vec![];
    let p_names = ["loop-1", "loop-2", "loop-3"];

    for name in p_names.iter() {
        let image_dir =
            env::temp_dir().join(format!("criu-e2e-test-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&image_dir);
        fs::create_dir_all(&image_dir).expect("Failed to create image directory");

        let dependencies: Vec<String> = p_names
            .iter()
            .filter(|&p| p != name)
            .map(|&s| s.to_string())
            .collect();

        // Create the configuration file for each process
        let config_path = image_dir.join("criu-coordinator.json");
        let mut config_file = fs::File::create(&config_path).expect("Failed to create config file");
        let config_content = format!(
            r#"{{
                "id": "{}",
                "dependencies": "{}",
                "address": "127.0.0.1",
                "port": "{}",
                "log-file": "coordinator.log"
            }}"#,
            name,
            dependencies.join(":"),
            port
        );
        config_file
            .write_all(config_content.as_bytes())
            .expect("Failed to write to config file");

        // Isolate the child process into its own process group.
        // This is to prevent the PGID of the test runner from colliding with the PGID
        // that CRIU is trying to restore. `setsid` creates a new session
        // and sets the process group ID.
        let mut command = Command::new(format!("./tests/{name}"));
        command
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .stdin(Stdio::null());

        unsafe {
            command.pre_exec(|| {
                // `setsid` should returns -1 on error.
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }

        let child = command.spawn().unwrap_or_else(|_| panic!("Failed to spawn process {}", name));
        let pid = child.id();

        processes.push(TestProcess {
            id: name.to_string(),
            child: Some(child),
            pid,
            image_dir,
            _dependencies: dependencies,
        });
        println!(
            "Spawned '{name}' with PID {pid}",
        );
    }
    thread::sleep(Duration::from_millis(500));
    processes
}

fn cleanup(server: &mut Child, processes: &mut [TestProcess]) {
    println!("\n--- Cleaning up test environment ---");
    let _ = server.kill();
    let _ = server.wait();
    println!("Killed server process.");

    for p in processes.iter_mut() {
        if let Some(mut child) = p.child.take() {
            // Because the child is in its own process group, killing the child
            // directly might not kill its children if it spawned any.
            // So we send signal to the whole process group.
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.wait();
            println!("Killed original process group for {} (PGID: {})", p.id, p.pid);
        }

        if let Some(pid) = get_pid_by_name(&p.id) {
            unsafe {
                libc::kill(pid as i32, libc::SIGKILL);
            }
            println!("Killed lingering process {} (PID: {})", p.id, pid);
        }
        let _ = fs::remove_dir_all(&p.image_dir);
    }

    let make_clean_status = Command::new("make")
        .arg("clean")
        .current_dir("tests")
        .status()
        .expect("Failed to run `make clean`");
    assert!(make_clean_status.success(), "make clean failed");

    println!("Cleanup complete.");
}

fn setup_tcp_test(coordinator_port: u16, tcp_server_port: u16) -> Vec<TestProcess> {
    println!("\n--- Setting up TCP client/server test environment ---");

    // Ensure the test binaries are compiled
    let make_status = Command::new("make")
        .current_dir("tests")
        .status()
        .expect("Failed to run `make` in tests directory");
    assert!(make_status.success(), "make command failed");

    let mut processes = Vec::new();

    // Setup for TCP Server
    let server_id = "tcp-server";
    let server_image_dir = env::temp_dir().join(format!(
        "criu-e2e-test-{}-{}",
        server_id,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&server_image_dir);
    fs::create_dir_all(&server_image_dir).expect("Failed to create server image directory");

    let server_config_path = server_image_dir.join("criu-coordinator.json");
    let mut server_config_file =
        fs::File::create(&server_config_path).expect("Failed to create server config file");
    let server_config_content = format!(
        r#"{{
            "id": "{server_id}",
            "dependencies": "tcp-client",
            "address": "127.0.0.1",
            "port": "{coordinator_port}",
            "log-file": "coordinator.log"
        }}"#
    );
    server_config_file
        .write_all(server_config_content.as_bytes())
        .expect("Failed to write to server config file");

    let mut server_command = Command::new("./tests/tcp-server");
    server_command
        .arg(tcp_server_port.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null());
    unsafe {
        server_command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let server_child = server_command.spawn().expect("Failed to spawn tcp-server");
    let server_pid = server_child.id();
    println!("Spawned '{server_id}' with PID {server_pid}");

    assert!(
        server_ready(&format!("127.0.0.1:{tcp_server_port}"), 20),
        "TCP server failed to start"
    );

    processes.push(TestProcess {
        id: server_id.to_string(),
        child: Some(server_child),
        pid: server_pid,
        image_dir: server_image_dir,
        _dependencies: vec!["tcp-client".to_string()],
    });

    // Setup for TCP Client
    let client_id = "tcp-client";
    let client_image_dir = env::temp_dir().join(format!(
        "criu-e2e-test-{}-{}",
        client_id,
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&client_image_dir);
    fs::create_dir_all(&client_image_dir).expect("Failed to create client image directory");

    let client_config_path = client_image_dir.join("criu-coordinator.json");
    let mut client_config_file =
        fs::File::create(&client_config_path).expect("Failed to create client config file");
    let client_config_content = format!(
        r#"{{
            "id": "{client_id}",
            "dependencies": "tcp-server",
            "address": "127.0.0.1",
            "port": "{coordinator_port}",
            "log-file": "coordinator.log"
        }}"#
    );
    client_config_file
        .write_all(client_config_content.as_bytes())
        .expect("Failed to write to client config file");

    let mut client_command = Command::new("./tests/tcp-client");
    client_command
        .args(["127.0.0.1", &tcp_server_port.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null());
    unsafe {
        client_command.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let client_child = client_command.spawn().expect("Failed to spawn tcp-client");
    let client_pid = client_child.id();
    println!("Spawned '{client_id}' with PID {client_pid}");

    processes.push(TestProcess {
        id: client_id.to_string(),
        child: Some(client_child),
        pid: client_pid,
        image_dir: client_image_dir,
        _dependencies: vec!["tcp-server".to_string()],
    });

    thread::sleep(Duration::from_millis(500));
    processes
}

#[test]
#[ignore] // requires require root privileges (make test-e2e)
fn e2e_dump_and_restore_with_criu() {
    assert!(
        is_root(),
        "This test must be run with root privileges for 'criu'."
    );

    assert!(is_criu_installed(), "CRIU command not found in PATH");

    let coordinator_path = fs::canonicalize("target/debug/criu-coordinator")
        .expect("Could not find criu-coordinator binary. Run 'cargo build' first.")
        .to_str()
        .unwrap()
        .to_owned();

    let port = pick_port();
    let addr = format!("127.0.0.1:{port}");
    let server = spawn_server(port);
    assert!(server_ready(&addr, 20), "Server failed to start at {}", addr);

    let processes = setup(port);
    let mut _guard = TestGuard { server, processes };

    println!("\n--- Starting checkpoint phase (concurrent) ---");
    let mut dump_handles = vec![];
    for p in &_guard.processes {
        let coordinator_path_clone = coordinator_path.clone();
        let p_id = p.id.clone();
        let p_pid = p.pid;
        let p_image_dir = p.image_dir.clone();
        dump_handles.push(thread::spawn(move || {
            let out = Command::new("sudo")
                .args([
                    "criu", "dump", "-t", &p_pid.to_string(), "-D",
                    p_image_dir.to_str().unwrap(), "-j", "-v4", "--action-script",
                    &coordinator_path_clone,
                ])
                .output()
                .expect("failed to execute criu");
            (p_id, out)
        }));
    }

    for handle in dump_handles {
        let (id, output) = handle.join().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success() && stderr.contains("Dumping finished successfully"),
            "CRIU failed for process '{}'.\nStderr:\n{}", id, stderr
        );
        println!("Checkpoint successful for {id}");
    }

    println!("\n--- REAPING checkpointed processes ---");
    for p in &mut _guard.processes {
        if let Some(mut child) = p.child.take() {
            // Wait for the original child process to be killed by CRIU.
            match child.wait() {
                Ok(status) => println!(
                    "Reaped process {} (PID {}) with exit status: {}",
                    p.id, p.pid, status
                ),
                Err(e) => eprintln!("Error reaping process {} (PID {}): {}", p.id, p.pid, e),
            }
        }
    }

    thread::sleep(Duration::from_millis(500));
    println!("\n--- Starting restore phase (concurrent) ---");
    let mut restore_handles = vec![];
    for p in &_guard.processes {
        let coordinator_path_clone = coordinator_path.clone();
        let p_id = p.id.clone();
        let p_image_dir = p.image_dir.clone();
        restore_handles.push(thread::spawn(move || {
            let out = Command::new("sudo")
                .args([
                    "criu", "restore", "-D", p_image_dir.to_str().unwrap(),
                    "-d",
                    "-v4", "--action-script", &coordinator_path_clone,
                ])
                .output()
                .expect("failed to execute criu restore");
            (p_id, out)
        }));
    }

    for handle in restore_handles {
        let (id, output) = handle.join().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success() && stderr.contains("Restore finished successfully"),
            "CRIU restore failed for process '{}'.\nStderr:\n{}", id, stderr
        );
        println!("Restore successful for {id}");
    }

    thread::sleep(Duration::from_millis(500));

    println!("\n--- VERIFYING restored processes ---");
    for p in &_guard.processes {
        assert!(
            get_pid_by_name(&p.id).is_some(),
            "Process {} was not found running after restore.",
            p.id
        );
        println!("Verified process {} is running.", p.id);
    }
}

#[test]
#[ignore] // requires require root privileges (make test-e2e)
fn e2e_dump_and_restore_tcp_client_server() {
    assert!(
        is_root(),
        "This test must be run with root privileges for 'criu'."
    );
    assert!(is_criu_installed(), "CRIU command not found in PATH");

    let coordinator_path = fs::canonicalize("target/debug/criu-coordinator")
        .expect("Could not find criu-coordinator binary. Run 'cargo build' first.")
        .to_str()
        .unwrap()
        .to_owned();

    let coordinator_port = pick_port();
    let coordinator_addr = format!("127.0.0.1:{coordinator_port}");
    let server = spawn_server(coordinator_port);
    assert!(
        server_ready(&coordinator_addr, 20),
        "Coordinator server failed to start at {}",
        coordinator_addr
    );

    let tcp_server_port = pick_port();
    let processes = setup_tcp_test(coordinator_port, tcp_server_port);
    let mut _guard = TestGuard { server, processes };

    thread::sleep(Duration::from_secs(10));

    println!("\n--- Starting checkpoint phase for TCP client/server ---");
    let mut dump_handles = vec![];
    for p in &_guard.processes {
        let coordinator_path_clone = coordinator_path.clone();
        let p_id = p.id.clone();
        let p_pid = p.pid;
        let p_image_dir = p.image_dir.clone();
        dump_handles.push(thread::spawn(move || {
            let out = Command::new("sudo")
                .args([
                    "criu",
                    "dump",
                    "-t",
                    &p_pid.to_string(),
                    "-D",
                    p_image_dir.to_str().unwrap(),
                    "-j",
                    "-v4",
                    "--tcp-established",
                    "--network-lock",
                    "iptables",
                    "--action-script",
                    &coordinator_path_clone,
                ])
                .output()
                .expect("failed to execute criu");
            (p_id, out)
        }));
    }

    for handle in dump_handles {
        let (id, output) = handle.join().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success() && stderr.contains("Dumping finished successfully"),
            "CRIU failed for process '{}'.\nStderr:\n{}",
            id,
            stderr
        );
        println!("Checkpoint successful for {id}");
    }

    println!("\n--- Reaping checkpointed processes ---");
    for p in &mut _guard.processes {
        if let Some(mut child) = p.child.take() {
            match child.wait() {
                Ok(status) => println!(
                    "Reaped process {} (PID {}) with exit status: {}",
                    p.id, p.pid, status
                ),
                Err(e) => eprintln!("Error reaping process {} (PID {}): {}", p.id, p.pid, e),
            }
        }
    }

    thread::sleep(Duration::from_millis(500));

    println!("\n--- Starting restore phase for TCP client/server ---");
    let mut restore_handles = vec![];
    for p in &_guard.processes {
        let coordinator_path_clone = coordinator_path.clone();
        let p_id = p.id.clone();
        let p_image_dir = p.image_dir.clone();

        restore_handles.push(thread::spawn(move || {
            let out = Command::new("sudo")
                .args([
                    "criu",
                    "restore",
                    "-D",
                    p_image_dir.to_str().unwrap(),
                    "--tcp-established",
                    "-d",
                    "-v4",
                    "--action-script",
                    &coordinator_path_clone,
                ])
                .output()
                .expect("failed to execute criu restore");
            (p_id, out)
        }));
    }

    for handle in restore_handles {
        let (id, output) = handle.join().unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success() && stderr.contains("Restore finished successfully"),
            "CRIU restore failed for process '{}'.\nStderr:\n{}",
            id,
            stderr
        );
        println!("Restore successful for {id}");
    }

    thread::sleep(Duration::from_millis(500));

    println!("\n--- Checking restored processes ---");
    for p in &_guard.processes {
        assert!(
            get_pid_by_name(&p.id).is_some(),
            "Process {} was not found running after restore.", p.id
        );
        println!("Verified process {} is running.", p.id);
    }
}
