//! Process-level regressions for clap's CLI exit status and output streams.
use std::process::{Command, Output};

fn invoke(args: &[&str]) -> Output {
    let root = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_kcode"))
        .current_dir(root.path())
        .env_clear()
        .env("HOME", root.path())
        .env("JCODE_HOME", root.path().join("jcode"))
        .env("JCODE_NO_TELEMETRY", "1")
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("run jcode CLI");
    assert!(
        !root.path().join("jcode").exists(),
        "parsing must exit before creating application state"
    );
    output
}

/// Strip stderr lines that macOS `codesign`/`xattr` emit when the freshly
/// built binary is ad-hoc signed on first execution. They are a property of
/// the build host, not of the CLI, and they make a strict "stderr is empty"
/// assertion fail on macOS while passing on Linux CI.
fn stderr_excluding_codesign_noise(bytes: &[u8]) -> String {
    let bin = env!("CARGO_BIN_EXE_kcode");
    String::from_utf8_lossy(bytes)
        .lines()
        .filter(|line| {
            if line.starts_with("xattr: ") || line.contains(": replacing existing signature") {
                return false;
            }
            // Parallel tests launch the same binary at once; the ad-hoc
            // re-sign step races and reports the path as missing.
            if line.starts_with(bin) && line.ends_with(": No such file or directory") {
                return false;
            }
            true
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn unknown_subcommand_exits_with_usage_error() {
    let output = invoke(&["definitely-not-a-command"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("unrecognized subcommand 'definitely-not-a-command'"));
    assert!(stderr.contains("Usage:"));
}

#[test]
fn unknown_option_exits_with_usage_error() {
    let output = invoke(&["--definitely-not-an-option"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("unexpected argument '--definitely-not-an-option'"));
    assert!(stderr.contains("Usage:"));
}

#[test]
fn help_exits_successfully_on_stdout() {
    let output = invoke(&["--help"]);
    assert_eq!(output.status.code(), Some(0));
    let stderr = stderr_excluding_codesign_noise(&output.stderr);
    assert!(stderr.is_empty(), "unexpected stderr: {stderr:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("Commands:"));
}

#[test]
fn version_exits_successfully_on_stdout() {
    let output = invoke(&["--version"]);
    assert_eq!(output.status.code(), Some(0));
    let stderr = stderr_excluding_codesign_noise(&output.stderr);
    assert!(stderr.is_empty(), "unexpected stderr: {stderr:?}");
    // The product was renamed: the root package is `kcode` and its bin target
    // is `kcode`, so `--version` reports `kcode <semver> (...)`. Asserting the
    // old `jcode ` prefix made this test fail against the shipped binary.
    assert!(
        String::from_utf8(output.stdout)
            .unwrap()
            .starts_with("kcode ")
    );
}

/// Regression: `kcode repl` must exit when stdin reaches EOF.
///
/// `read_line` returns `Ok(0)` on EOF and leaves the buffer empty. Treating
/// that as a blank line made the prompt loop retry forever, so the process
/// never terminated. Observed on a real Windows run (killed at 45 s) and
/// reproduced on macOS with `< /dev/null`.
///
/// The process is spawned with `Stdio::null()` for stdin, which is an
/// immediate EOF, and then polled against a deadline. A hanging build fails
/// with the prompt count instead of hanging the test suite.
#[test]
fn repl_exits_on_stdin_eof() {
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let root = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_kcode"))
        .current_dir(root.path())
        .env_clear()
        .env("HOME", root.path())
        .env("JCODE_HOME", root.path().join("jcode"))
        .env("JCODE_NO_TELEMETRY", "1")
        .env("NO_COLOR", "1")
        // A dummy key so provider autodetection succeeds offline. Without one,
        // `repl` exits 1 in its onboarding/login path before the prompt loop
        // is ever entered, and the test would prove nothing about EOF.
        .env("ANTHROPIC_API_KEY", "sk-ant-dummy-not-a-real-key")
        .arg("repl")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn kcode repl");

    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        match child.try_wait().expect("poll kcode repl") {
            Some(status) => {
                let output = child.wait_with_output().expect("collect output");
                let stdout = String::from_utf8_lossy(&output.stdout);
                let prompts = stdout.matches("> ").count();
                assert!(
                    prompts > 0,
                    "repl never reached its prompt loop, so this run proves nothing about EOF \
                     handling; stdout was {stdout:?}"
                );
                assert!(
                    status.success(),
                    "repl must exit cleanly on stdin EOF, got {status:?}; stderr: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                return;
            }
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let output = child.wait_with_output().expect("collect output");
                let stdout = String::from_utf8_lossy(&output.stdout);
                panic!(
                    "repl did not exit within 30s of stdin EOF: the prompt loop is retrying on \
                     an empty buffer. Saw {} prompt(s) and {} bytes of stdout, so it reached the \
                     loop and kept going.",
                    stdout.matches("> ").count(),
                    output.stdout.len()
                );
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}
