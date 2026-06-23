use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rustix::process::{kill_process, Pid, Signal};

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let unique = format!(
        "{}-{}-{}",
        prefix,
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos()
    );
    let path = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&path).expect("temporary test directory should be created");
    path
}

fn write_settings_file(path: &Path) {
    std::fs::write(
        path,
        "[settings]\nsession_timeout = 10\nlogin_retry_times = 5\nlogin_retry_window = 600\n\n[root]\nsalt = parity-salt\npassword = ignored\npermission = status_networking networking_connections networking_edit networking_activate networking_ap_activate networking_certificates system_user system_settings system_firmware system_logs\n",
    )
    .expect("settings file should be written");
}

fn generate_tls_assets(cert_path: &Path, key_path: &Path) {
    let status = Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-keyout",
            key_path.to_str().expect("key path should be utf-8"),
            "-out",
            cert_path.to_str().expect("cert path should be utf-8"),
            "-days",
            "1",
            "-subj",
            "/CN=127.0.0.1",
            "-addext",
            "subjectAltName=DNS:localhost,IP:127.0.0.1",
            "-addext",
            "extendedKeyUsage=serverAuth",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("openssl should run");
    assert!(status.success(), "openssl should generate test TLS assets");
}

fn write_server_config(path: &Path, cert_path: &Path, key_path: &Path) {
    std::fs::write(
        path,
        format!(
            "[/]\ntools.sessions.on = false\n\n[plugins]\n\n[summit-rcm]\ndefault_username = root\ndefault_password = summit\nallow_multiple_user_sessions = true\nnetwork_status_restricted = false\nlog_routes_loaded = false\n\n[global]\nserver.ssl_private_key = {}\nserver.ssl_certificate = {}\nserver.ssl_certificate_chain = {}\n",
            key_path.display(),
            cert_path.display(),
            cert_path.display(),
        ),
    )
    .expect("server config should be written");
}

fn summit_rcm_binary() -> PathBuf {
    if let Some(binary) = std::env::var_os("CARGO_BIN_EXE_summit-rcm") {
        return PathBuf::from(binary);
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let binary_path = manifest_dir.join("target").join("debug").join("summit-rcm");
    if binary_path.is_file() {
        return binary_path;
    }

    let status = Command::new("cargo")
        .args(["build", "--quiet", "--bin", "summit-rcm"])
        .current_dir(&manifest_dir)
        .status()
        .expect("cargo build should run for shutdown integration test");
    assert!(status.success(), "cargo build should produce summit-rcm binary for shutdown integration test");
    assert!(binary_path.is_file(), "summit-rcm binary should exist after cargo build at {}", binary_path.display());
    binary_path
}

fn reserve_bind_addr() -> String {
    let listener = TcpListener::bind("127.0.0.1:0")
        .expect("test should reserve an ephemeral TCP port");
    let addr = listener
        .local_addr()
        .expect("reserved listener should expose a local address");
    drop(listener);
    addr.to_string()
}

fn wait_for_listening(child: &mut Child, bind_addr: &str, timeout: Duration) {
    let deadline = std::time::Instant::now() + timeout;

    while std::time::Instant::now() < deadline {
        if TcpStream::connect(bind_addr).is_ok() {
            return;
        }
        if let Some(status) = child.try_wait().expect("child status should be readable") {
            panic!("child exited before listening: {status}");
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    panic!("timed out waiting for child process to start listening");
}

#[test]
fn shutdown_signal_returns_on_sigterm() {
    let temp_dir = unique_temp_dir("summit-rcm-shutdown-test");
    let settings_path = temp_dir.join("summit-rcm-settings.ini");
    let server_config_path = temp_dir.join("summit-rcm.ini");
    let cert_path = temp_dir.join("server.crt");
    let key_path = temp_dir.join("server.key");

    write_settings_file(&settings_path);
    generate_tls_assets(&cert_path, &key_path);
    write_server_config(&server_config_path, &cert_path, &key_path);
    let bind_addr = reserve_bind_addr();

    let binary = summit_rcm_binary();

    let mut child = Command::new(&binary)
        .env("SUMMIT_RCM_SERVER_CONF_FILE", &server_config_path)
        .env("SUMMIT_RCM_SETTINGS_FILE", &settings_path)
        .env("SUMMIT_RCM_BIND", &bind_addr)
        .env("RUST_LOG", "summit_rcm=info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("summit-rcm binary should start");

    wait_for_listening(&mut child, &bind_addr, Duration::from_secs(5));

    let pid = i32::try_from(child.id())
        .ok()
        .and_then(Pid::from_raw)
        .expect("child pid should fit in rustix::process::Pid");
    kill_process(pid, Signal::TERM).expect("SIGTERM should be sent to child");

    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait().expect("child status should be readable") {
            assert!(
                status.success(),
                "child should exit cleanly after SIGTERM, got status {status:?}"
            );
            break;
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("timed out waiting for child process to exit after SIGTERM");
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    std::fs::remove_dir_all(&temp_dir).expect("temporary test directory should be removed");
}
