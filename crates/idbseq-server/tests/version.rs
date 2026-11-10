use std::process::Command;

#[test]
fn prints_binary_name_and_version() {
    // Arrange
    let expected = format!("idbseqd {}\n", env!("CARGO_PKG_VERSION"));

    // Act
    let output = Command::new(env!("CARGO_BIN_EXE_idbseqd"))
        .arg("--version")
        .output()
        .expect("idbseqd should run");

    // Assert
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), expected);
}
