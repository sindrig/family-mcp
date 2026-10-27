use std::process::Command;

#[test]
fn version_help_and_invalid_cli_are_offline() {
    let bin = env!("CARGO_BIN_EXE_rjupa-mcp");
    let version = Command::new(bin).arg("--version").output().unwrap();
    assert!(version.status.success());
    assert_eq!(
        String::from_utf8(version.stdout).unwrap().trim(),
        env!("CARGO_PKG_VERSION")
    );
    let help = Command::new(bin).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(
        String::from_utf8(help.stdout)
            .unwrap()
            .contains("stdio MCP server")
    );
    assert!(
        !Command::new(bin)
            .arg("--unknown")
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[cfg(not(feature = "test-origin"))]
#[test]
fn shipping_binary_has_no_test_environment_hooks() {
    let binary = std::fs::read(env!("CARGO_BIN_EXE_rjupa-mcp")).unwrap();
    for name in [b"RJUPA_TEST_ORIGIN".as_slice(), b"RJUPA_TEST_NOW"] {
        assert!(!binary.windows(name.len()).any(|bytes| bytes == name));
    }
}
