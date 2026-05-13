use std::io::Write;
use std::process::{Command, Stdio};

use skycode_orchestrator::orchestrator::task_loop::build_rewrite_patch;

#[test]
fn new_file_diff_has_new_file_mode() {
    let patch = build_rewrite_patch("CHANGELOG.md", "", "hello\nworld\n");

    assert!(patch.contains("diff --git a/CHANGELOG.md b/CHANGELOG.md"));
    assert!(patch.contains("new file mode 100644"));
    assert!(patch.contains("--- /dev/null"));
    assert!(patch.contains("+++ b/CHANGELOG.md"));
    assert!(patch.contains("@@ -0,0 +1,2 @@"));
}

#[test]
fn new_file_diff_applies_via_git() {
    let repo = tempfile::tempdir().expect("create temp repo");
    run_git(repo.path(), &["init", "-q"]);
    std::fs::write(repo.path().join("README.md"), "initial\n").expect("write seed file");
    run_git(repo.path(), &["add", "README.md"]);
    run_git(
        repo.path(),
        &[
            "-c",
            "user.name=SkyCode Test",
            "-c",
            "user.email=skycode@example.invalid",
            "commit",
            "-m",
            "initial",
            "--quiet",
        ],
    );

    let patch = build_rewrite_patch("CHANGELOG.md", "", "hello\nworld\n");
    let mut child = Command::new("git")
        .arg("-C")
        .arg(repo.path())
        .arg("apply")
        .arg("--check")
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn git apply --check");

    let mut stdin = child.stdin.take().expect("git apply stdin");
    stdin
        .write_all(patch.as_bytes())
        .expect("write patch to git apply");
    drop(stdin);

    let output = child.wait_with_output().expect("wait for git apply");
    assert!(
        output.status.success(),
        "git apply --check failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn existing_file_diff_unchanged() {
    let patch = build_rewrite_patch("CHANGELOG.md", "alpha\nbeta\n", "alpha\ngamma\n");

    assert!(!patch.contains("new file mode"));
    assert!(!patch.contains("--- /dev/null"));
}

fn run_git(repo: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .expect("run git");

    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}
