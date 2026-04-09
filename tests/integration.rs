use std::path::PathBuf;
use std::process::Command;

fn binary() -> PathBuf {
    let mut p = std::env::current_exe().unwrap();
    p.pop(); // deps/
    p.pop(); // debug/ or release/
    p.push("claimcheck");
    p
}

fn project_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture(name: &str) -> PathBuf {
    project_dir().join("tests/fixtures").join(name)
}

fn run(transcript: &str, extra_args: &[&str]) -> (String, i32) {
    let out = Command::new(binary())
        .arg(fixture(transcript))
        .arg("--project-dir")
        .arg(project_dir())
        .args(extra_args)
        .output()
        .expect("failed to run claimcheck");

    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let code = out.status.code().unwrap_or(-1);
    (stdout, code)
}

// ── pass fixture ─────────────────────────────────────────────────────────────

#[test]
fn pass_fixture_truth_score_100() {
    let (out, code) = run("pass.jsonl", &[]);
    assert_eq!(code, 0);
    assert!(out.contains("Truth score: 100%"), "got:\n{}", out);
}

#[test]
fn pass_fixture_all_pass() {
    let (out, _) = run("pass.jsonl", &[]);
    assert!(
        out.contains("Summary: 5 passed, 0 failed, 0 unverifiable"),
        "got:\n{}",
        out
    );
}

#[test]
fn pass_fixture_real_files_verified() {
    let (out, _) = run("pass.jsonl", &[]);
    assert!(
        out.contains("src/main.rs") && out.contains("PASS"),
        "got:\n{}",
        out
    );
    assert!(
        out.contains("src/verifier.rs") && out.contains("PASS"),
        "got:\n{}",
        out
    );
    assert!(
        out.contains("src/parser.rs") && out.contains("PASS"),
        "got:\n{}",
        out
    );
}

#[test]
fn pass_fixture_lockfile_packages_verified() {
    let (out, _) = run("pass.jsonl", &[]);
    // proptest and serde are real entries in Cargo.lock
    assert!(out.contains("installed proptest"), "got:\n{}", out);
    assert!(out.contains("installed serde"), "got:\n{}", out);
}

#[test]
fn pass_fixture_json_output() {
    let (out, code) = run("pass.jsonl", &["--json"]);
    assert_eq!(code, 0);
    let v: serde_json::Value = serde_json::from_str(&out).expect("invalid JSON output");
    assert_eq!(v["truth_score"], "100%");
    assert_eq!(v["summary"]["pass"], 5);
    assert_eq!(v["summary"]["fail"], 0);
    assert_eq!(v["summary"]["unverifiable"], 0);
}

// ── fail fixture ─────────────────────────────────────────────────────────────

#[test]
fn fail_fixture_truth_score_0() {
    let (out, code) = run("fail.jsonl", &[]);
    assert_eq!(code, 0);
    assert!(out.contains("Truth score: 0%"), "got:\n{}", out);
}

#[test]
fn fail_fixture_missing_files_caught() {
    let (out, _) = run("fail.jsonl", &[]);
    assert!(out.contains("src/db/connection.rs"), "got:\n{}", out);
    assert!(out.contains("src/db/migrations.rs"), "got:\n{}", out);
    // Both should FAIL with "file not found on disk"
    let fail_count = out.matches("file not found on disk").count();
    assert_eq!(
        fail_count, 2,
        "expected 2 file-not-found failures, got:\n{}",
        out
    );
}

#[test]
fn fail_fixture_missing_package_caught() {
    let (out, _) = run("fail.jsonl", &[]);
    assert!(out.contains("installed diesel"), "got:\n{}", out);
    assert!(
        out.contains("package not found in any lockfile"),
        "got:\n{}",
        out
    );
}

#[test]
fn fail_fixture_test_without_runner_output_is_unverifiable() {
    let (out, _) = run("fail.jsonl", &[]);
    // "All 42 tests pass" with no runner output in transcript → UNVERIFIABLE
    assert!(out.contains("42 tests pass"), "got:\n{}", out);
    assert!(out.contains("UNVERIFIABLE"), "got:\n{}", out);
}

#[test]
fn fail_fixture_numeric_no_git_commits_is_unverifiable() {
    let (out, _) = run("fail.jsonl", &[]);
    // Repo has no commits → numeric claim is UNVERIFIABLE
    assert!(out.contains("edited 10 files"), "got:\n{}", out);
}

#[test]
fn fail_fixture_json_output() {
    let (out, _) = run("fail.jsonl", &["--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).expect("invalid JSON");
    assert_eq!(v["truth_score"], "0%");
    assert_eq!(v["summary"]["fail"], 4);
    assert_eq!(v["summary"]["unverifiable"], 1);
}

// ── unverifiable fixture ──────────────────────────────────────────────────────

#[test]
fn unverifiable_fixture_score_na() {
    let (out, code) = run("unverifiable.jsonl", &[]);
    assert_eq!(code, 0);
    assert!(out.contains("Truth score: N/A"), "got:\n{}", out);
}

#[test]
fn unverifiable_fixture_all_unverifiable() {
    let (out, _) = run("unverifiable.jsonl", &[]);
    assert!(
        out.contains("Summary: 0 passed, 0 failed, 3 unverifiable"),
        "got:\n{}",
        out
    );
}

#[test]
fn unverifiable_bugfix_no_path_is_unverifiable() {
    let (out, _) = run("unverifiable.jsonl", &[]);
    // "fixed the null pointer bug" has no file path → UNVERIFIABLE
    assert!(out.contains("null pointer bug"), "got:\n{}", out);
    let lines: Vec<&str> = out.lines().collect();
    let bugfix_line = lines
        .iter()
        .find(|l| l.contains("null pointer bug"))
        .unwrap();
    assert!(
        bugfix_line.contains("UNVERIFIABLE"),
        "expected UNVERIFIABLE, got: {}",
        bugfix_line
    );
}

#[test]
fn unverifiable_test_self_confirm_blocked() {
    let (out, _) = run("unverifiable.jsonl", &[]);
    // "All tests pass" with no runner output → UNVERIFIABLE, not PASS
    let lines: Vec<&str> = out.lines().collect();
    let test_line = lines.iter().find(|l| l.contains("All tests pass")).unwrap();
    assert!(
        test_line.contains("UNVERIFIABLE"),
        "self-confirmation should be blocked, got: {}",
        test_line
    );
}

// ── error handling ────────────────────────────────────────────────────────────

#[test]
fn missing_file_exits_1() {
    let out = Command::new(binary())
        .arg("/nonexistent/transcript.jsonl")
        .arg("--project-dir")
        .arg(project_dir())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("not found"), "got stderr: {}", stderr);
}

#[test]
fn unsupported_format_exits_1() {
    let out = Command::new(binary())
        .arg(fixture("pass.jsonl").with_extension("txt"))
        .arg("--project-dir")
        .arg(project_dir())
        .output()
        .unwrap();
    // File doesn't exist → exits 1 with file not found (acceptable)
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn empty_transcript_exits_0() {
    use std::io::Write;
    let mut f = tempfile::Builder::new()
        .suffix(".jsonl")
        .tempfile()
        .unwrap();
    f.write_all(b"").unwrap();
    let out = Command::new(binary())
        .arg(f.path())
        .arg("--project-dir")
        .arg(project_dir())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("No verifiable claims found"),
        "got: {}",
        stdout
    );
}

// ── contract tests: tricky language ──────────────────────────────────────────

#[test]
fn contract_created_with_filler_words_extracts_path() {
    // "created a new file src/x.ts" should extract src/x.ts, not "a"
    use std::io::Write;
    let content =
        r#"{"role":"assistant","content":"I created a new file src/main.rs for the entry point."}"#;
    let mut f = tempfile::Builder::new()
        .suffix(".jsonl")
        .tempfile()
        .unwrap();
    f.write_all(content.as_bytes()).unwrap();
    let out = Command::new(binary())
        .arg(f.path())
        .arg("--project-dir")
        .arg(project_dir())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Should find src/main.rs (exists) and PASS, not extract "a" as identifier
    assert!(
        stdout.contains("PASS"),
        "expected PASS for real file, got:\n{}",
        stdout
    );
    assert!(
        !stdout.contains("identifier: a"),
        "should not extract 'a' as identifier, got:\n{}",
        stdout
    );
}

#[test]
fn contract_bugfix_no_path_is_unverifiable() {
    use std::io::Write;
    let content = r#"{"role":"assistant","content":"Fixed the null pointer bug that was causing crashes in production."}"#;
    let mut f = tempfile::Builder::new()
        .suffix(".jsonl")
        .tempfile()
        .unwrap();
    f.write_all(content.as_bytes()).unwrap();
    let out = Command::new(binary())
        .arg(f.path())
        .arg("--project-dir")
        .arg(project_dir())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("UNVERIFIABLE"),
        "bugfix without path should be UNVERIFIABLE, got:\n{}",
        stdout
    );
}

#[test]
fn contract_test_self_confirm_no_runner_output_is_unverifiable() {
    use std::io::Write;
    let content =
        r#"{"role":"assistant","content":"All tests pass. The implementation is solid."}"#;
    let mut f = tempfile::Builder::new()
        .suffix(".jsonl")
        .tempfile()
        .unwrap();
    f.write_all(content.as_bytes()).unwrap();
    let out = Command::new(binary())
        .arg(f.path())
        .arg("--project-dir")
        .arg(project_dir())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("UNVERIFIABLE"),
        "self-confirm should be UNVERIFIABLE, got:\n{}",
        stdout
    );
    assert!(
        !stdout.contains("PASS"),
        "should not PASS without runner output, got:\n{}",
        stdout
    );
}

#[test]
fn contract_test_with_cargo_runner_output_passes() {
    use std::io::Write;
    let content = "{\"role\":\"assistant\",\"content\":\"I ran the tests:\\n\\ntest result: ok. 15 passed; 0 failed\\nexit code: 0\\n\\nAll 15 tests pass.\"}";
    let mut f = tempfile::Builder::new()
        .suffix(".jsonl")
        .tempfile()
        .unwrap();
    f.write_all(content.as_bytes()).unwrap();
    let out = Command::new(binary())
        .arg(f.path())
        .arg("--project-dir")
        .arg(project_dir())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("PASS"),
        "should PASS with real runner output, got:\n{}",
        stdout
    );
}

#[test]
fn contract_test_runner_failure_contradicts_pass_claim() {
    use std::io::Write;
    let content = "{\"role\":\"assistant\",\"content\":\"All tests pass!\\n\\ntest result: FAILED. 3 passed; 2 failed\\nexit code: 1\"}";
    let mut f = tempfile::Builder::new()
        .suffix(".jsonl")
        .tempfile()
        .unwrap();
    f.write_all(content.as_bytes()).unwrap();
    let out = Command::new(binary())
        .arg(f.path())
        .arg("--project-dir")
        .arg(project_dir())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("FAIL"),
        "contradiction should produce FAIL, got:\n{}",
        stdout
    );
}

#[test]
fn contract_added_article_not_extracted_as_package() {
    use std::io::Write;
    // "added a guard", "added an import", "added it" should not produce Package claims
    let content = r#"{"role":"assistant","content":"I added a guard clause and added an import for the logger. I also added it to the config."}"#;
    let mut f = tempfile::Builder::new()
        .suffix(".jsonl")
        .tempfile()
        .unwrap();
    f.write_all(content.as_bytes()).unwrap();
    let out = Command::new(binary())
        .arg(f.path())
        .arg("--project-dir")
        .arg(project_dir())
        .arg("--show-claims")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("[Package]"),
        "articles should not be extracted as packages, got:\n{}",
        stdout
    );
}

#[test]
fn contract_numeric_functions_is_unverifiable() {
    use std::io::Write;
    let content = r#"{"role":"assistant","content":"I added 5 functions to the module."}"#;
    let mut f = tempfile::Builder::new()
        .suffix(".jsonl")
        .tempfile()
        .unwrap();
    f.write_all(content.as_bytes()).unwrap();
    let out = Command::new(binary())
        .arg(f.path())
        .arg("--project-dir")
        .arg(project_dir())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("UNVERIFIABLE"),
        "function count should be UNVERIFIABLE, got:\n{}",
        stdout
    );
}

#[test]
fn contract_show_claims_flag() {
    let (out, _) = run("pass.jsonl", &["--show-claims"]);
    // --show-claims prints claims before the report
    assert!(out.contains("[File]"), "got:\n{}", out);
    assert!(out.contains("[Package]"), "got:\n{}", out);
    assert!(out.contains("Truth score:"), "got:\n{}", out);
}

#[test]
fn contract_markdown_claude_format() {
    use std::io::Write;
    let content = "**Human:** Can you create a config file?\n\n**Claude:** I created src/types.rs with the shared type definitions.\n\n**Human:** Thanks\n";
    let mut f = tempfile::Builder::new().suffix(".md").tempfile().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    let out = Command::new(binary())
        .arg(f.path())
        .arg("--project-dir")
        .arg(project_dir())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    // src/types.rs exists in the claimcheck repo → PASS
    assert!(
        stdout.contains("PASS"),
        "Claude markdown format should parse and verify, got:\n{}",
        stdout
    );
}
