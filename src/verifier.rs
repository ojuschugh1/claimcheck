use std::path::{Path, PathBuf};
use std::process::Command;

use crate::types::{Claim, ClaimType, FileOp, NumericMetric, VerificationResult, VerifiedClaim};
use crate::{git, lockfile};

pub struct VerifierConfig {
    pub project_dir: PathBuf,
    pub baseline: String,
    pub retest: bool,
    pub test_cmd: Option<String>,
    pub verbose: bool,
    pub transcript_text: Option<String>,
}

pub fn verify_claims(claims: &[Claim], config: &VerifierConfig) -> Vec<VerifiedClaim> {
    let is_git = git::is_git_repo(&config.project_dir);
    let has_commits = is_git && git::has_commits(&config.project_dir);

    // Run the test command once up front if --retest is set and any test claims exist.
    // Caching avoids re-running an expensive suite once per extracted test claim.
    let has_test_claims = claims.iter().any(|c| c.claim_type == ClaimType::Test);
    let retest_outcome: Option<RetestOutcome> = if config.retest && has_test_claims {
        let cmd = config
            .test_cmd
            .clone()
            .or_else(|| detect_test_command(&config.project_dir));
        Some(match cmd {
            Some(cmd) => run_test_command(&cmd, &config.project_dir, config.verbose),
            None => RetestOutcome::Undetectable,
        })
    } else {
        None
    };

    claims
        .iter()
        .map(|claim| VerifiedClaim {
            result: verify_one(claim, config, is_git, has_commits, retest_outcome.as_ref()),
            claim: claim.clone(),
        })
        .collect()
}

/// Outcome of a single test command execution, cached across all test claims.
enum RetestOutcome {
    Passed,
    Failed { exit_code: i32 },
    Undetectable,
    Error { reason: String },
}

fn verify_one(
    claim: &Claim,
    config: &VerifierConfig,
    is_git: bool,
    has_commits: bool,
    retest: Option<&RetestOutcome>,
) -> VerificationResult {
    match &claim.claim_type {
        ClaimType::File => verify_file(claim, config, is_git, has_commits),
        ClaimType::Package => verify_package(claim, config),
        ClaimType::Test => verify_test(claim, config, retest),
        ClaimType::BugFix => verify_bugfix(claim, config, is_git, has_commits),
        ClaimType::Numeric => verify_numeric(claim, config, is_git, has_commits),
    }
}

fn verify_file(
    claim: &Claim,
    config: &VerifierConfig,
    is_git: bool,
    has_commits: bool,
) -> VerificationResult {
    let id = match &claim.identifier {
        Some(id) => id,
        None => return unverifiable("no file path in claim"),
    };
    let op = match &claim.file_op {
        Some(op) => op,
        None => return unverifiable("no file operation in claim"),
    };
    let path = config.project_dir.join(id);

    match op {
        FileOp::Create => {
            if path.exists() {
                VerificationResult::Pass
            } else {
                fail("file not found on disk")
            }
        }
        FileOp::Delete => {
            if !path.exists() {
                VerificationResult::Pass
            } else {
                fail("file still exists on disk")
            }
        }
        FileOp::Modify => {
            if !is_git {
                return unverifiable("not a git repository");
            }
            if !has_commits {
                return if path.exists() {
                    VerificationResult::Pass
                } else {
                    fail("file not found on disk")
                };
            }
            if config.verbose {
                eprintln!("[verbose] git diff {} -- {}", config.baseline, id);
            }
            match git::file_changed(&config.project_dir, &config.baseline, id) {
                Ok(true) => VerificationResult::Pass,
                Ok(false) => fail(&format!("no changes to {} since {}", id, config.baseline)),
                Err(_) => unverifiable("git not available"),
            }
        }
    }
}

fn verify_package(claim: &Claim, config: &VerifierConfig) -> VerificationResult {
    let package = match &claim.identifier {
        Some(id) => id,
        None => return unverifiable("no package name in claim"),
    };

    let lockfiles = lockfile::find_lockfiles(&config.project_dir, 2);
    if config.verbose {
        eprintln!(
            "[verbose] scanning {} lockfile(s) for '{}'",
            lockfiles.len(),
            package
        );
    }
    if lockfiles.is_empty() {
        return unverifiable("no lockfile found in project");
    }

    for lf in &lockfiles {
        if lockfile::package_in_lockfile(lf, package).unwrap_or(false) {
            return VerificationResult::Pass;
        }
    }
    fail("package not found in any lockfile")
}

fn verify_test(
    claim: &Claim,
    config: &VerifierConfig,
    retest: Option<&RetestOutcome>,
) -> VerificationResult {
    if let Some(outcome) = retest {
        let claim_lower = claim.raw_text.to_lowercase();
        let claim_says_pass = claim_lower.contains("pass");
        let claim_says_fail = claim_lower.contains("fail");
        let claim_has_polarity = claim_says_pass || claim_says_fail;

        return match outcome {
            RetestOutcome::Undetectable => {
                unverifiable("could not detect a test command; use --test-cmd to specify one")
            }
            RetestOutcome::Error { reason } => {
                unverifiable(&format!("failed to run test command: {}", reason))
            }
            RetestOutcome::Passed => {
                if !claim_has_polarity {
                    return unverifiable("test claim has no pass/fail assertion");
                }
                if claim_says_fail {
                    fail("claim says tests fail but test command exited 0")
                } else {
                    VerificationResult::Pass
                }
            }
            RetestOutcome::Failed { exit_code } => {
                if !claim_has_polarity {
                    return unverifiable("test claim has no pass/fail assertion");
                }
                if claim_says_pass {
                    fail(&format!(
                        "claim says tests pass but test command exited {}",
                        exit_code
                    ))
                } else {
                    VerificationResult::Pass
                }
            }
        };
    }

    let evidence = config.transcript_text.as_deref().unwrap_or("");
    let runner_evidence = find_runner_output(evidence);

    if runner_evidence.is_empty() {
        return unverifiable("no test runner output found in transcript");
    }

    let runner_lower: String = runner_evidence
        .iter()
        .map(|s| s.to_lowercase())
        .collect::<Vec<_>>()
        .join("\n");

    let has_failure = runner_lower.contains("failures:")
        || runner_lower.contains("exit code: 1")
        || runner_lower.contains("exit code 1")
        || runner_lower.contains("error[");

    let has_nonzero_failure = runner_evidence.iter().any(|line| {
        let l = line.trim().to_lowercase();
        if l.contains("0 failed") {
            return false;
        }
        l.contains("failed") || l.contains("failure")
    });
    let has_failure = has_failure || has_nonzero_failure;

    let has_success = runner_lower.contains("test result: ok")
        || runner_lower.contains("exit code: 0")
        || runner_lower.contains("exit code 0")
        || (runner_lower.contains("passed") && !has_failure);

    let claim_lower = claim.raw_text.to_lowercase();
    let claim_says_pass = claim_lower.contains("pass");
    let claim_says_fail = claim_lower.contains("fail");

    if claim_says_pass && has_failure {
        return fail("claim says tests pass but transcript shows test failures");
    }
    if claim_says_fail && has_success && !has_failure {
        return fail("claim says tests fail but transcript shows tests passing");
    }
    if claim_says_pass && has_success {
        return VerificationResult::Pass;
    }
    if claim_says_fail && has_failure {
        return VerificationResult::Pass;
    }

    unverifiable("cannot determine test result from transcript")
}

/// Detect the test command from project files.
/// Checks for Cargo.toml, package.json, pytest, go.mod, Makefile in that order.
pub fn detect_test_command(dir: &Path) -> Option<String> {
    if dir.join("Cargo.toml").exists() {
        return Some("cargo test".to_string());
    }
    if dir.join("package.json").exists() {
        // Prefer npm test; yarn if yarn.lock present
        if dir.join("yarn.lock").exists() {
            return Some("yarn test".to_string());
        }
        if dir.join("pnpm-lock.yaml").exists() {
            return Some("pnpm test".to_string());
        }
        return Some("npm test".to_string());
    }
    if dir.join("go.mod").exists() {
        return Some("go test ./...".to_string());
    }
    if dir.join("setup.py").exists() || dir.join("pyproject.toml").exists() {
        return Some("pytest".to_string());
    }
    if dir.join("Gemfile").exists() {
        return Some("bundle exec rspec".to_string());
    }
    if dir.join("Makefile").exists() {
        return Some("make test".to_string());
    }
    None
}

fn run_test_command(cmd: &str, dir: &Path, verbose: bool) -> RetestOutcome {
    if verbose {
        eprintln!("[verbose] running test command: {}", cmd);
    }

    // Use `sh -c` so quoted args, pipes, and shell operators work correctly.
    let result = Command::new("sh")
        .args(["-c", cmd])
        .current_dir(dir)
        .output();

    match result {
        Ok(output) => {
            if verbose {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stdout.is_empty() {
                    eprintln!("[verbose] stdout:\n{}", stdout);
                }
                if !stderr.is_empty() {
                    eprintln!("[verbose] stderr:\n{}", stderr);
                }
                eprintln!(
                    "[verbose] exit code: {}",
                    output.status.code().unwrap_or(-1)
                );
            }
            if output.status.success() {
                RetestOutcome::Passed
            } else {
                RetestOutcome::Failed {
                    exit_code: output.status.code().unwrap_or(-1),
                }
            }
        }
        Err(e) => {
            if verbose {
                eprintln!("[verbose] failed to spawn sh: {}", e);
            }
            RetestOutcome::Error {
                reason: e.to_string(),
            }
        }
    }
}

/// Extract lines that look like actual test runner output, not natural language.
fn find_runner_output(transcript: &str) -> Vec<&str> {
    transcript
        .lines()
        .filter(|line| {
            let l = line.trim().to_lowercase();
            l.starts_with("test result:")
                || l.starts_with("tests:")
                || l.starts_with("test suite")
                || l.contains("exit code")
                || l.contains("passed,")
                || l.contains("failed,")
                || l.starts_with("failures:")
                || l.starts_with("ok.")
                || l.starts_with("fail")
                || l.contains("error[")
                || (l.contains(" passed")
                    && l.chars()
                        .next()
                        .map(|c| c.is_ascii_digit())
                        .unwrap_or(false))
                || (l.contains(" failed")
                    && l.chars()
                        .next()
                        .map(|c| c.is_ascii_digit())
                        .unwrap_or(false))
        })
        .collect()
}

fn verify_bugfix(
    claim: &Claim,
    config: &VerifierConfig,
    is_git: bool,
    has_commits: bool,
) -> VerificationResult {
    let id = match &claim.identifier {
        Some(id) => id,
        None => return unverifiable("no file reference in bug fix claim"),
    };

    if !is_git {
        return unverifiable("not a git repository");
    }

    if !has_commits {
        let path = config.project_dir.join(id);
        return if path.exists() {
            VerificationResult::Pass
        } else {
            fail("file not found on disk")
        };
    }

    if config.verbose {
        eprintln!("[verbose] git diff {} -- {}", config.baseline, id);
    }
    match git::file_changed(&config.project_dir, &config.baseline, id) {
        Ok(true) => VerificationResult::Pass,
        Ok(false) => fail(&format!("no changes to {} since {}", id, config.baseline)),
        Err(_) => unverifiable("git not available"),
    }
}

fn verify_numeric(
    claim: &Claim,
    config: &VerifierConfig,
    is_git: bool,
    has_commits: bool,
) -> VerificationResult {
    let claimed = match claim.numeric_value {
        Some(n) => n,
        None => return unverifiable("no count in claim"),
    };

    match &claim.numeric_metric {
        Some(NumericMetric::FilesEdited) => {}
        Some(NumericMetric::Functions) => {
            return unverifiable("function count cannot be verified from git")
        }
        Some(NumericMetric::Lines) => return unverifiable("line count verification not supported"),
        None => return unverifiable("unknown numeric metric"),
    }

    if !is_git {
        return unverifiable("not a git repository");
    }
    if !has_commits {
        return unverifiable("git repository has no commits");
    }

    if config.verbose {
        eprintln!("[verbose] git diff --stat {}", config.baseline);
    }
    match git::diff_stat(&config.project_dir, &config.baseline) {
        Ok(stat) => {
            let actual = stat.files_changed as u64;
            if actual == claimed {
                VerificationResult::Pass
            } else {
                fail(&format!(
                    "claimed {} files but git shows {} files changed",
                    claimed, actual
                ))
            }
        }
        Err(_) => unverifiable("git not available"),
    }
}

fn fail(reason: &str) -> VerificationResult {
    VerificationResult::Fail {
        reason: reason.to_string(),
    }
}

fn unverifiable(reason: &str) -> VerificationResult {
    VerificationResult::Unverifiable {
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Claim, ClaimType, FileOp, NumericMetric};
    use proptest::prelude::*;
    use std::fs;
    use tempfile::TempDir;

    fn file_claim(op: FileOp, id: Option<&str>) -> Claim {
        Claim {
            claim_type: ClaimType::File,
            raw_text: "created src/foo.rs".into(),
            identifier: id.map(|s| s.to_string()),
            file_op: Some(op),
            numeric_value: None,
            numeric_metric: None,
        }
    }

    fn pkg_claim(id: Option<&str>) -> Claim {
        Claim {
            claim_type: ClaimType::Package,
            raw_text: "installed serde".into(),
            identifier: id.map(|s| s.to_string()),
            file_op: None,
            numeric_value: None,
            numeric_metric: None,
        }
    }

    fn test_claim(raw: &str) -> Claim {
        Claim {
            claim_type: ClaimType::Test,
            raw_text: raw.into(),
            identifier: None,
            file_op: None,
            numeric_value: None,
            numeric_metric: None,
        }
    }

    fn cfg(dir: &TempDir) -> VerifierConfig {
        VerifierConfig {
            project_dir: dir.path().into(),
            baseline: "HEAD".into(),
            retest: false,
            test_cmd: None,
            verbose: false,
            transcript_text: None,
        }
    }

    fn cfg_with_transcript(dir: &TempDir, transcript: &str) -> VerifierConfig {
        VerifierConfig {
            project_dir: dir.path().into(),
            baseline: "HEAD".into(),
            retest: false,
            test_cmd: None,
            verbose: false,
            transcript_text: Some(transcript.to_string()),
        }
    }

    #[test]
    fn create_pass_when_exists() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("foo.rs"), "").unwrap();
        assert_eq!(
            verify_file(
                &file_claim(FileOp::Create, Some("foo.rs")),
                &cfg(&dir),
                false,
                false
            ),
            VerificationResult::Pass
        );
    }

    #[test]
    fn create_fail_when_missing() {
        let dir = TempDir::new().unwrap();
        assert!(matches!(
            verify_file(
                &file_claim(FileOp::Create, Some("nope.rs")),
                &cfg(&dir),
                false,
                false
            ),
            VerificationResult::Fail { .. }
        ));
    }

    #[test]
    fn delete_pass_when_absent() {
        let dir = TempDir::new().unwrap();
        assert_eq!(
            verify_file(
                &file_claim(FileOp::Delete, Some("gone.rs")),
                &cfg(&dir),
                false,
                false
            ),
            VerificationResult::Pass
        );
    }

    #[test]
    fn delete_fail_when_present() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("here.rs"), "").unwrap();
        assert!(matches!(
            verify_file(
                &file_claim(FileOp::Delete, Some("here.rs")),
                &cfg(&dir),
                false,
                false
            ),
            VerificationResult::Fail { .. }
        ));
    }

    #[test]
    fn modify_unverifiable_without_git() {
        let dir = TempDir::new().unwrap();
        assert!(matches!(
            verify_file(
                &file_claim(FileOp::Modify, Some("foo.rs")),
                &cfg(&dir),
                false,
                false
            ),
            VerificationResult::Unverifiable { .. }
        ));
    }

    #[test]
    fn no_identifier_unverifiable() {
        let dir = TempDir::new().unwrap();
        assert!(matches!(
            verify_file(&file_claim(FileOp::Create, None), &cfg(&dir), false, false),
            VerificationResult::Unverifiable { .. }
        ));
    }

    #[test]
    fn package_no_lockfile() {
        let dir = TempDir::new().unwrap();
        assert!(matches!(
            verify_package(&pkg_claim(Some("serde")), &cfg(&dir)),
            VerificationResult::Unverifiable { .. }
        ));
    }

    #[test]
    fn package_found() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.lock"), "name = \"serde\"\n").unwrap();
        assert_eq!(
            verify_package(&pkg_claim(Some("serde")), &cfg(&dir)),
            VerificationResult::Pass
        );
    }

    #[test]
    fn package_not_found() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.lock"), "name = \"tokio\"\n").unwrap();
        assert!(matches!(
            verify_package(&pkg_claim(Some("serde")), &cfg(&dir)),
            VerificationResult::Fail { .. }
        ));
    }

    #[test]
    fn test_pass_with_runner_output() {
        let dir = TempDir::new().unwrap();
        let c = cfg_with_transcript(&dir, "test result: ok. 5 passed; 0 failed\nexit code: 0");
        assert_eq!(
            verify_test(&test_claim("all tests pass"), &c, None),
            VerificationResult::Pass
        );
    }

    #[test]
    fn test_fail_claim_contradicts_runner_output() {
        let dir = TempDir::new().unwrap();
        let c = cfg_with_transcript(&dir, "2 failed, 3 passed\nexit code: 1");
        assert!(matches!(
            verify_test(&test_claim("all tests pass"), &c, None),
            VerificationResult::Fail { .. }
        ));
    }

    #[test]
    fn test_unverifiable_claim_only_no_runner_output() {
        let dir = TempDir::new().unwrap();
        let c = cfg_with_transcript(&dir, "I ran the tests and all tests pass.");
        assert!(matches!(
            verify_test(&test_claim("all tests pass"), &c, None),
            VerificationResult::Unverifiable { .. }
        ));
    }

    #[test]
    fn test_unverifiable_no_transcript() {
        let dir = TempDir::new().unwrap();
        assert!(matches!(
            verify_test(&test_claim("all tests pass"), &cfg(&dir), None),
            VerificationResult::Unverifiable { .. }
        ));
    }

    // Retest tests go through verify_claims so the caching + polarity logic is exercised end-to-end
    #[test]
    fn test_retest_pass_claim_command_exits_0() {
        let dir = TempDir::new().unwrap();
        let c = VerifierConfig {
            project_dir: dir.path().into(),
            baseline: "HEAD".into(),
            retest: true,
            test_cmd: Some("true".to_string()),
            verbose: false,
            transcript_text: None,
        };
        let results = verify_claims(&[test_claim("all tests pass")], &c);
        assert_eq!(results[0].result, VerificationResult::Pass);
    }

    #[test]
    fn test_retest_pass_claim_command_exits_1() {
        let dir = TempDir::new().unwrap();
        let c = VerifierConfig {
            project_dir: dir.path().into(),
            baseline: "HEAD".into(),
            retest: true,
            test_cmd: Some("false".to_string()),
            verbose: false,
            transcript_text: None,
        };
        let results = verify_claims(&[test_claim("all tests pass")], &c);
        assert!(matches!(results[0].result, VerificationResult::Fail { .. }));
    }

    #[test]
    fn test_retest_fail_claim_command_exits_1() {
        // Claim says "tests failed", command exits 1 → PASS (claim is correct)
        let dir = TempDir::new().unwrap();
        let c = VerifierConfig {
            project_dir: dir.path().into(),
            baseline: "HEAD".into(),
            retest: true,
            test_cmd: Some("false".to_string()),
            verbose: false,
            transcript_text: None,
        };
        let results = verify_claims(&[test_claim("tests failed")], &c);
        assert_eq!(results[0].result, VerificationResult::Pass);
    }

    #[test]
    fn test_retest_fail_claim_command_exits_0() {
        // Claim says "tests failed", command exits 0 → FAIL (claim is wrong)
        let dir = TempDir::new().unwrap();
        let c = VerifierConfig {
            project_dir: dir.path().into(),
            baseline: "HEAD".into(),
            retest: true,
            test_cmd: Some("true".to_string()),
            verbose: false,
            transcript_text: None,
        };
        let results = verify_claims(&[test_claim("tests failed")], &c);
        assert!(matches!(results[0].result, VerificationResult::Fail { .. }));
    }

    #[test]
    fn test_retest_runs_once_for_multiple_claims() {
        // Two test claims with --retest should produce consistent results
        // (both see the same cached outcome, not two separate runs)
        let dir = TempDir::new().unwrap();
        let c = VerifierConfig {
            project_dir: dir.path().into(),
            baseline: "HEAD".into(),
            retest: true,
            test_cmd: Some("true".to_string()),
            verbose: false,
            transcript_text: None,
        };
        let claims = vec![test_claim("all tests pass"), test_claim("ran 5 tests")];
        let results = verify_claims(&claims, &c);
        assert_eq!(results[0].result, VerificationResult::Pass);
        assert!(matches!(
            results[1].result,
            VerificationResult::Unverifiable { .. }
        ));
    }

    #[test]
    fn test_retest_shell_quoted_args() {
        // sh -c handles quoted args correctly
        let dir = TempDir::new().unwrap();
        let c = VerifierConfig {
            project_dir: dir.path().into(),
            baseline: "HEAD".into(),
            retest: true,
            test_cmd: Some("sh -c 'exit 0'".to_string()),
            verbose: false,
            transcript_text: None,
        };
        let results = verify_claims(&[test_claim("all tests pass")], &c);
        assert_eq!(results[0].result, VerificationResult::Pass);
    }

    #[test]
    fn test_retest_no_cmd_no_project_files_unverifiable() {
        let dir = TempDir::new().unwrap();
        let c = VerifierConfig {
            project_dir: dir.path().into(),
            baseline: "HEAD".into(),
            retest: true,
            test_cmd: None,
            verbose: false,
            transcript_text: None,
        };
        let results = verify_claims(&[test_claim("all tests pass")], &c);
        assert!(matches!(
            results[0].result,
            VerificationResult::Unverifiable { .. }
        ));
    }

    #[test]
    fn test_retest_is_skipped_when_no_test_claims() {
        // With --retest enabled but no test claims, we should not run test commands.
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("foo.rs"), "").unwrap();
        let c = VerifierConfig {
            project_dir: dir.path().into(),
            baseline: "HEAD".into(),
            retest: true,
            test_cmd: Some("false".to_string()),
            verbose: false,
            transcript_text: None,
        };
        let claim = Claim {
            claim_type: ClaimType::File,
            raw_text: "created foo.rs".into(),
            identifier: Some("foo.rs".into()),
            file_op: Some(FileOp::Create),
            numeric_value: None,
            numeric_metric: None,
        };
        let results = verify_claims(&[claim], &c);
        assert_eq!(results[0].result, VerificationResult::Pass);
    }

    #[test]
    fn detect_cargo_project() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        assert_eq!(
            detect_test_command(dir.path()),
            Some("cargo test".to_string())
        );
    }

    #[test]
    fn detect_npm_project() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        assert_eq!(
            detect_test_command(dir.path()),
            Some("npm test".to_string())
        );
    }

    #[test]
    fn detect_yarn_project() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("package.json"), "{}").unwrap();
        fs::write(dir.path().join("yarn.lock"), "").unwrap();
        assert_eq!(
            detect_test_command(dir.path()),
            Some("yarn test".to_string())
        );
    }

    #[test]
    fn detect_go_project() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("go.mod"), "module example.com/app").unwrap();
        assert_eq!(
            detect_test_command(dir.path()),
            Some("go test ./...".to_string())
        );
    }

    #[test]
    fn detect_pytest_project() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("pyproject.toml"), "[tool.pytest]").unwrap();
        assert_eq!(detect_test_command(dir.path()), Some("pytest".to_string()));
    }

    #[test]
    fn detect_no_project_returns_none() {
        let dir = TempDir::new().unwrap();
        assert_eq!(detect_test_command(dir.path()), None);
    }

    #[test]
    fn bugfix_no_file_path_is_unverifiable() {
        let dir = TempDir::new().unwrap();
        let claim = Claim {
            claim_type: ClaimType::BugFix,
            raw_text: "fixed the null pointer bug".into(),
            identifier: None,
            file_op: None,
            numeric_value: None,
            numeric_metric: None,
        };
        assert!(matches!(
            verify_bugfix(&claim, &cfg(&dir), true, true),
            VerificationResult::Unverifiable { .. }
        ));
    }

    #[test]
    fn bugfix_with_file_path_no_git() {
        let dir = TempDir::new().unwrap();
        let claim = Claim {
            claim_type: ClaimType::BugFix,
            raw_text: "fixed src/foo.rs".into(),
            identifier: Some("src/foo.rs".into()),
            file_op: None,
            numeric_value: None,
            numeric_metric: None,
        };
        assert!(matches!(
            verify_bugfix(&claim, &cfg(&dir), false, false),
            VerificationResult::Unverifiable { .. }
        ));
    }

    #[test]
    fn numeric_files_edited_no_git() {
        let dir = TempDir::new().unwrap();
        let claim = Claim {
            claim_type: ClaimType::Numeric,
            raw_text: "edited 3 files".into(),
            identifier: Some("3".into()),
            file_op: None,
            numeric_value: Some(3),
            numeric_metric: Some(NumericMetric::FilesEdited),
        };
        assert!(matches!(
            verify_numeric(&claim, &cfg(&dir), false, false),
            VerificationResult::Unverifiable { .. }
        ));
    }

    #[test]
    fn numeric_functions_always_unverifiable() {
        let dir = TempDir::new().unwrap();
        let claim = Claim {
            claim_type: ClaimType::Numeric,
            raw_text: "added 5 functions".into(),
            identifier: Some("5".into()),
            file_op: None,
            numeric_value: Some(5),
            numeric_metric: Some(NumericMetric::Functions),
        };
        assert!(matches!(
            verify_numeric(&claim, &cfg(&dir), true, true),
            VerificationResult::Unverifiable { .. }
        ));
    }

    #[test]
    fn numeric_lines_always_unverifiable() {
        let dir = TempDir::new().unwrap();
        let claim = Claim {
            claim_type: ClaimType::Numeric,
            raw_text: "changed 100 lines".into(),
            identifier: Some("100".into()),
            file_op: None,
            numeric_value: Some(100),
            numeric_metric: Some(NumericMetric::Lines),
        };
        assert!(matches!(
            verify_numeric(&claim, &cfg(&dir), true, true),
            VerificationResult::Unverifiable { .. }
        ));
    }

    #[test]
    fn verify_claims_file_modify_no_git() {
        let dir = TempDir::new().unwrap();
        let claim = Claim {
            claim_type: ClaimType::File,
            raw_text: "modified src/main.rs".into(),
            identifier: Some("src/main.rs".into()),
            file_op: Some(FileOp::Modify),
            numeric_value: None,
            numeric_metric: None,
        };
        let results = verify_claims(&[claim], &cfg(&dir));
        assert!(matches!(
            results[0].result,
            VerificationResult::Unverifiable { .. }
        ));
    }

    #[test]
    fn verify_claims_package_no_lockfiles() {
        let dir = TempDir::new().unwrap();
        let claim = Claim {
            claim_type: ClaimType::Package,
            raw_text: "installed express".into(),
            identifier: Some("express".into()),
            file_op: None,
            numeric_value: None,
            numeric_metric: None,
        };
        let results = verify_claims(&[claim], &cfg(&dir));
        assert!(matches!(
            results[0].result,
            VerificationResult::Unverifiable { .. }
        ));
    }

    fn safe_filename() -> impl Strategy<Value = String> {
        (
            proptest::char::ranges(std::borrow::Cow::Borrowed(&[('a'..='z')])),
            proptest::collection::vec(
                proptest::char::ranges(std::borrow::Cow::Borrowed(&[
                    ('a'..='z'),
                    ('0'..='9'),
                    ('_'..='_'),
                ])),
                0..=15,
            ),
        )
            .prop_map(|(first, rest)| {
                let mut name = first.to_string();
                name.extend(rest);
                name.push_str(".rs");
                name
            })
    }

    proptest! {
        #[test]
        fn prop_create_exists_pass(filename in safe_filename()) {
            let dir = TempDir::new().unwrap();
            fs::write(dir.path().join(&filename), b"").unwrap();
            let claim = Claim { claim_type: ClaimType::File, raw_text: format!("created {}", filename), identifier: Some(filename), file_op: Some(FileOp::Create), numeric_value: None, numeric_metric: None };
            prop_assert_eq!(verify_file(&claim, &cfg(&dir), false, false), VerificationResult::Pass);
        }

        #[test]
        fn prop_create_absent_fail(filename in safe_filename()) {
            let dir = TempDir::new().unwrap();
            let claim = Claim { claim_type: ClaimType::File, raw_text: format!("created {}", filename), identifier: Some(filename), file_op: Some(FileOp::Create), numeric_value: None, numeric_metric: None };
            let is_fail = matches!(verify_file(&claim, &cfg(&dir), false, false), VerificationResult::Fail { .. });
            prop_assert!(is_fail);
        }

        #[test]
        fn prop_delete_absent_pass(filename in safe_filename()) {
            let dir = TempDir::new().unwrap();
            let claim = Claim { claim_type: ClaimType::File, raw_text: format!("deleted {}", filename), identifier: Some(filename), file_op: Some(FileOp::Delete), numeric_value: None, numeric_metric: None };
            prop_assert_eq!(verify_file(&claim, &cfg(&dir), false, false), VerificationResult::Pass);
        }

        #[test]
        fn prop_delete_exists_fail(filename in safe_filename()) {
            let dir = TempDir::new().unwrap();
            fs::write(dir.path().join(&filename), b"").unwrap();
            let claim = Claim { claim_type: ClaimType::File, raw_text: format!("deleted {}", filename), identifier: Some(filename), file_op: Some(FileOp::Delete), numeric_value: None, numeric_metric: None };
            let is_fail = matches!(verify_file(&claim, &cfg(&dir), false, false), VerificationResult::Fail { .. });
            prop_assert!(is_fail);
        }
    }
}
