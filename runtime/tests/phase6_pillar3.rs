use skycode_runtime::tools::verify::run_verify;

#[test]
fn phase6_verify_pass() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    let outcome = run_verify(tmp.path(), pass_cmd(), 5)?;

    assert_eq!(outcome.exit_code, 0);
    assert!(!outcome.timed_out);

    Ok(())
}

#[test]
fn phase6_verify_fail_nonzero() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    let outcome = run_verify(tmp.path(), fail_cmd(), 5)?;

    assert_eq!(outcome.exit_code, 1);
    assert!(!outcome.timed_out);

    Ok(())
}

#[test]
fn phase6_verify_env_stripped() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    std::env::set_var("SKYCODE_TEST_SECRET", "xsecretx");

    let outcome = run_verify(tmp.path(), echo_secret_to_stderr_cmd(), 5);

    std::env::remove_var("SKYCODE_TEST_SECRET");
    let outcome = outcome?;

    assert!(
        !outcome.stderr_truncated.contains("xsecretx"),
        "SKYCODE_* env var leaked into verify stderr"
    );
    assert!(!outcome.timed_out);

    Ok(())
}

#[test]
fn phase6_verify_timeout() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = tempfile::tempdir()?;
    let outcome = run_verify(tmp.path(), sleep_ten_cmd(), 1)?;

    assert_eq!(outcome.exit_code, -1);
    assert!(outcome.timed_out);

    Ok(())
}

#[cfg(windows)]
fn pass_cmd() -> &'static str {
    "exit /B 0"
}

#[cfg(not(windows))]
fn pass_cmd() -> &'static str {
    "exit 0"
}

#[cfg(windows)]
fn fail_cmd() -> &'static str {
    "exit /B 1"
}

#[cfg(not(windows))]
fn fail_cmd() -> &'static str {
    "exit 1"
}

#[cfg(windows)]
fn echo_secret_to_stderr_cmd() -> &'static str {
    "echo %SKYCODE_TEST_SECRET% 1>&2"
}

#[cfg(not(windows))]
fn echo_secret_to_stderr_cmd() -> &'static str {
    "printf '%s' \"$SKYCODE_TEST_SECRET\" 1>&2"
}

#[cfg(windows)]
fn sleep_ten_cmd() -> &'static str {
    "ping 127.0.0.1 -n 11 >NUL"
}

#[cfg(not(windows))]
fn sleep_ten_cmd() -> &'static str {
    "sleep 10"
}
