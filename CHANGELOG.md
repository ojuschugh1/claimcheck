# Changelog

## Unreleased

### Added
- Initial implementation of the full verification pipeline
- JSONL transcript parsing (Claude Code native format)
- Markdown transcript parsing with support for `## Assistant`, `**Assistant**`, `**Assistant:**`, `**Claude:**`, `## Claude` markers and inline content on the marker line
- Claim extraction via compiled regex patterns for File, Package, Test, BugFix, and Numeric claim types
- File claim verification against the filesystem (`Path::exists()`)
- File modification verification via `git diff` against a configurable baseline
- Package claim verification by scanning lockfiles up to 2 directory levels deep
- Test claim verification against structured runner output in the transcript (cargo test, jest, pytest, go test patterns)
- Bug fix claim verification via `git diff` when a file path is present in the description
- Numeric file-count claim verification via `git diff --stat`
- `NumericMetric` enum to distinguish file-count claims (verifiable) from function/line claims (UNVERIFIABLE)
- Truth score calculation: `PASS / (PASS + FAIL) × 100`, UNVERIFIABLE excluded
- Plain text and JSON report output
- `--show-claims` flag to inspect extracted claims before verification
- `--baseline` flag to configure the git session window
- `--project-dir` flag to point at a project other than cwd
- `--json` flag for machine-readable output
- Property-based tests (proptest) for all 9 correctness properties
- Integration test suite against real fixture transcripts and real project files
- Contract tests for tricky extraction cases (filler words, articles, self-confirmation)
- Graceful handling of non-git directories, empty repos, and empty transcripts

### Known limitations
- `--retest` flag is accepted but not yet implemented (returns UNVERIFIABLE)
- Function and line count claims are always UNVERIFIABLE
- Bug fix claims without an explicit file path are UNVERIFIABLE
