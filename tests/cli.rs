use std::process::Command;

fn cli(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_orbweaver"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn discoverable_commands() {
    let r = cli(&["--help"]);
    assert!(r.status.success());
    let help = String::from_utf8(r.stdout).unwrap();
    for command in ["devices", "inspect", "monitor", "probe", "permissions"] {
        assert!(help.contains(command));
    }
}

#[test]
fn rejects_unsupported_interfaces_before_access() {
    for args in [
        vec!["probe", "--interface", "3"],
        vec!["monitor", "--interface", "-1"],
        vec!["monitor", "--seconds", "no"],
    ] {
        let r = cli(&args);
        assert_eq!(r.status.code(), Some(2));
    }
}

#[test]
fn recording_cannot_overwrite_existing_file() {
    let path = std::env::temp_dir().join(format!("orbweaver-test-{}.jsonl", std::process::id()));
    let original = b"existing recording\n";
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    std::io::Write::write_all(&mut file, original).unwrap();
    drop(file);
    let result = cli(&[
        "monitor",
        "--seconds",
        "1",
        "--output",
        path.to_str().unwrap(),
    ]);
    let after = std::fs::read(&path).unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(!result.status.success());
    assert_eq!(after, original);
    assert!(String::from_utf8_lossy(&result.stderr).contains("never overwritten"));
}
