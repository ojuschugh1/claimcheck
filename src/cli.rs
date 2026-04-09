use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "claimcheck",
    about = "Verify AI agent claims from session transcripts"
)]
pub struct Cli {
    /// Path to the transcript file (.jsonl or .md/.markdown)
    pub transcript: PathBuf,

    /// Git baseline ref for session window
    #[arg(long, default_value = "HEAD")]
    pub baseline: String,

    /// Re-run tests to verify test claims. Auto-detects the test command
    /// from the project (cargo test, npm test, pytest, go test, etc.)
    /// or use --test-cmd to specify one explicitly.
    #[arg(long)]
    pub retest: bool,

    /// Explicit test command to run when --retest is set (e.g. "cargo test")
    #[arg(long)]
    pub test_cmd: Option<String>,

    /// Output report as JSON
    #[arg(long)]
    pub json: bool,

    /// Project root directory (default: cwd)
    #[arg(long)]
    pub project_dir: Option<PathBuf>,

    /// Show extracted claims before verification
    #[arg(long)]
    pub show_claims: bool,

    /// Print git commands and other diagnostic info
    #[arg(long)]
    pub verbose: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcript_is_required() {
        assert!(Cli::try_parse_from(["claimcheck"]).is_err());
    }

    #[test]
    fn transcript_positional() {
        let cli = Cli::try_parse_from(["claimcheck", "transcript.jsonl"]).unwrap();
        assert_eq!(cli.transcript, PathBuf::from("transcript.jsonl"));
    }

    #[test]
    fn baseline_flag() {
        let cli = Cli::try_parse_from(["claimcheck", "t.jsonl", "--baseline", "abc123"]).unwrap();
        assert_eq!(cli.baseline, "abc123");
    }

    #[test]
    fn baseline_defaults_to_head() {
        let cli = Cli::try_parse_from(["claimcheck", "t.jsonl"]).unwrap();
        assert_eq!(cli.baseline, "HEAD");
    }

    #[test]
    fn retest_flag() {
        let cli = Cli::try_parse_from(["claimcheck", "t.jsonl", "--retest"]).unwrap();
        assert!(cli.retest);
    }

    #[test]
    fn test_cmd_flag() {
        let cli = Cli::try_parse_from([
            "claimcheck",
            "t.jsonl",
            "--retest",
            "--test-cmd",
            "cargo test",
        ])
        .unwrap();
        assert_eq!(cli.test_cmd, Some("cargo test".to_string()));
    }

    #[test]
    fn json_flag() {
        let cli = Cli::try_parse_from(["claimcheck", "t.jsonl", "--json"]).unwrap();
        assert!(cli.json);
    }

    #[test]
    fn project_dir_flag() {
        let cli =
            Cli::try_parse_from(["claimcheck", "t.jsonl", "--project-dir", "/some/path"]).unwrap();
        assert_eq!(cli.project_dir, Some(PathBuf::from("/some/path")));
    }

    #[test]
    fn show_claims_flag() {
        let cli = Cli::try_parse_from(["claimcheck", "t.jsonl", "--show-claims"]).unwrap();
        assert!(cli.show_claims);
    }

    #[test]
    fn verbose_flag() {
        let cli = Cli::try_parse_from(["claimcheck", "t.jsonl", "--verbose"]).unwrap();
        assert!(cli.verbose);
    }

    #[test]
    fn all_flags_together() {
        let cli = Cli::try_parse_from([
            "claimcheck",
            "session.jsonl",
            "--baseline",
            "main",
            "--retest",
            "--test-cmd",
            "npm test",
            "--json",
            "--project-dir",
            "/workspace",
            "--show-claims",
            "--verbose",
        ])
        .unwrap();
        assert_eq!(cli.transcript, PathBuf::from("session.jsonl"));
        assert_eq!(cli.baseline, "main");
        assert!(cli.retest);
        assert_eq!(cli.test_cmd, Some("npm test".to_string()));
        assert!(cli.json);
        assert_eq!(cli.project_dir, Some(PathBuf::from("/workspace")));
        assert!(cli.show_claims);
        assert!(cli.verbose);
    }

    #[test]
    fn defaults_when_only_transcript() {
        let cli = Cli::try_parse_from(["claimcheck", "t.jsonl"]).unwrap();
        assert_eq!(cli.baseline, "HEAD");
        assert!(!cli.retest);
        assert!(cli.test_cmd.is_none());
        assert!(!cli.json);
        assert!(cli.project_dir.is_none());
        assert!(!cli.show_claims);
        assert!(!cli.verbose);
    }
}
