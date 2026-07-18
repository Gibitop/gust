use std::process::Command;

#[test]
fn help_prints_usage_and_exits_successfully() {
    let output = Command::new(env!("CARGO_BIN_EXE_gustc"))
        .arg("--help")
        .output()
        .expect("gustc --help should run");

    assert!(output.status.success());
    assert!(output.stderr.is_empty());

    let stdout = String::from_utf8(output.stdout).expect("help output should be utf-8");
    assert!(stdout.contains("usage: gustc <file.gust|directory> [options]"));
    assert!(stdout.contains("--emit-c <output.c>"));
    assert!(stdout.contains("--std-path <path>"));
    assert!(stdout.contains("--gc-stress"));
    assert!(stdout.contains("--help"));
}
