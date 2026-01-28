//! Test directory management for ACP integration tests.

use std::fs;
use std::path::PathBuf;

/// Paths for a single test run, isolated from other tests.
pub struct TestPaths {
    pub base_dir: PathBuf,
    pub cwd: PathBuf,
    pub sessions_dir: PathBuf,
    pub agents_dir: PathBuf,
    pub settings_path: PathBuf,
    pub ipc_socket: PathBuf,
    pub log_file: PathBuf,
}

/// Create isolated test directories under test_output/{test_name}/.
pub fn create_test_dir(test_name: &str) -> TestPaths {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let base_dir = crate_dir.join("test_output").join(test_name);

    // Clean and recreate
    let _ = fs::remove_dir_all(&base_dir);
    fs::create_dir_all(&base_dir).expect("failed to create test dir");

    let cwd = base_dir.join("cwd");
    let sessions_dir = base_dir.join("sessions");
    let agents_dir = base_dir.join("agents");

    fs::create_dir_all(&cwd).expect("failed to create test cwd");
    fs::create_dir_all(&sessions_dir).expect("failed to create test sessions dir");
    fs::create_dir_all(&agents_dir).expect("failed to create test agents dir");

    let settings_path = base_dir.join("settings.json");
    // Initialize empty settings file
    fs::write(&settings_path, "{}").expect("failed to create settings file");

    // Unix sockets have a max path length of 104 bytes on macOS. Use /tmp to avoid hitting this limit.
    let ipc_socket = PathBuf::from(format!("/tmp/{}.sock", test_name));
    // Clean up stale socket from previous runs
    let _ = fs::remove_file(&ipc_socket);

    TestPaths {
        cwd: cwd.clone(),
        sessions_dir,
        agents_dir,
        settings_path,
        ipc_socket,
        log_file: base_dir.join("agent.log"),
        base_dir,
    }
}
