//! The compiled `idbseq` binary: version, help and exit statuses.

use std::io::Write;
use std::process::{Command, Stdio};

fn idbseq() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_idbseq"));
    command
        .env_remove("IDBSEQ_URL")
        .env_remove("IDBSEQ_KEY")
        .env("IDBSEQ_CONFIG_DIR", env!("CARGO_TARGET_TMPDIR"));
    command
}

#[test]
fn prints_binary_name_and_version() {
    let output = idbseq().arg("--version").output().unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        format!("idbseq {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn long_help_documents_exit_codes() {
    let output = idbseq().arg("--help").output().unwrap();

    let help = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(help.contains("Exit codes:"));
    assert!(help.contains("12  aborted"));
}

#[test]
fn usage_errors_exit_2() {
    let output = idbseq().args(["next"]).output().unwrap();

    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn network_errors_exit_3() {
    let output = idbseq()
        .args([
            "--url",
            "http://127.0.0.1:9",
            "--timeout",
            "2",
            "last",
            "orders",
        ])
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(3));
}

#[test]
fn confirmation_reads_standard_input() {
    // Arrange: nothing listens on port 9, so a confirmed delete fails with a
    // network error (3) while a declined one aborts (12) before connecting.
    let run = |answer: &str| {
        let mut child = idbseq()
            .args([
                "--url",
                "http://127.0.0.1:9",
                "--timeout",
                "2",
                "collections",
                "delete",
                "orders",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(answer.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    };

    // Act
    let declined = run("no\n");
    let confirmed = run("orders\n");
    let closed = run("");

    // Assert
    assert_eq!(declined.status.code(), Some(12));
    assert!(String::from_utf8_lossy(&declined.stderr).contains("Type `orders` to confirm"));
    assert_eq!(confirmed.status.code(), Some(3));
    assert_eq!(closed.status.code(), Some(12));
}

#[test]
fn login_reads_the_password_from_piped_input() {
    let dir = tempfile::tempdir().unwrap();
    let mut child = idbseq()
        .env("IDBSEQ_CONFIG_DIR", dir.path())
        .args(["--url", "http://127.0.0.1:9", "--timeout", "2", "login"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"secret\n").unwrap();

    let output = child.wait_with_output().unwrap();

    assert_eq!(output.status.code(), Some(3));
}
