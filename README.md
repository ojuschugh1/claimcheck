# claimcheck

[![crates.io](https://img.shields.io/crates/v/claimcheck.svg)](https://crates.io/crates/claimcheck)
[![CI](https://github.com/ojuschugh1/claimcheck/actions/workflows/ci.yml/badge.svg)](https://github.com/ojuschugh1/claimcheck/actions/workflows/ci.yml)

Verify whether an AI coding agent actually did what it claimed.

claimcheck parses an agent session transcript, extracts every concrete claim the agent made (files created, packages installed, tests run, bugs fixed, counts of changes), and checks each one against the real filesystem, git history, and lockfiles. The result is a truth score and a per-claim PASS / FAIL / UNVERIFIABLE report.

No LLM calls. No API keys. Everything runs locally.

---

## Why

AI agents frequently overclaim. They say "I created src/auth.ts" when the file doesn't exist, "all tests pass" when no test runner was invoked, or "I installed express" when it never made it into the lockfile. claimcheck makes these lies visible.

---

## Install

```sh
cargo install claimcheck
```

Requires Rust 1.70+ and `git` in PATH. If you don't have Rust installed, get it from [rustup.rs](https://rustup.rs).

Or build from source:

```sh
git clone https://github.com/ojuschugh1/claimcheck
cd claimcheck
cargo build --release
# binary at target/release/claimcheck
```

---

## Usage

```
claimcheck [OPTIONS] <TRANSCRIPT>

Arguments:
  <TRANSCRIPT>  Path to the transcript file (.jsonl or .md / .markdown)

Options:
      --baseline <REF>       Git baseline for session window [default: HEAD]
      --project-dir <DIR>    Project root directory [default: cwd]
      --retest               Re-run tests to verify test claims (auto-detects test command)
      --test-cmd <CMD>       Explicit test command for --retest (e.g. "cargo test")
      --show-claims          Print extracted claims before running verification
      --json                 Output report as JSON
      --verbose              Print git commands and diagnostic info
  -h, --help                 Print help
```

### Basic

```sh
claimcheck session.jsonl
```

### Point at a specific project

```sh
claimcheck session.jsonl --project-dir ~/code/myapp
```

### Scope the session window to a specific commit range

```sh
claimcheck session.jsonl --baseline main
claimcheck session.jsonl --baseline HEAD~3
```

### Re-run tests live

```sh
# Auto-detect test command from project files (Cargo.toml, package.json, go.mod, etc.)
claimcheck session.jsonl --retest

# Specify the command explicitly
claimcheck session.jsonl --retest --test-cmd "cargo test --release"
```

### Debug what's happening

```sh
claimcheck session.jsonl --verbose
```

### See what was extracted before verifying

```sh
claimcheck session.jsonl --show-claims
```

### Machine-readable output

```sh
claimcheck session.jsonl --json | jq '.summary'
```

---

## Supported transcript formats

### Claude Code JSONL (`.jsonl`)

The native export format from Claude Code sessions.

```jsonl
{"role": "user", "content": "Add authentication"}
{"role": "assistant", "content": "I created src/auth.ts with JWT validation."}
```

### Cursor JSONL (`.jsonl`)

Cursor composer and chat exports are also supported:

```jsonl
{"type": "assistant", "text": "I created src/auth.ts"}
{"role": "assistant", "parts": [{"type": "text", "text": "I installed axios"}]}
```

### Markdown (`.md`, `.markdown`)

Exported conversation logs. Supports multiple heading styles:

```markdown
## User
Add authentication

## Assistant
I created src/auth.ts with JWT validation.
```

```markdown
**Human:** Add authentication

**Claude:** I created src/auth.ts with JWT validation.
```

```markdown
**Assistant:** I created src/auth.ts with JWT validation.
```

Inline content on the marker line (`**Claude:** I created ...`) is captured correctly.

---

## What gets verified

| Claim type | Example | Verification method |
|---|---|---|
| File created | "created src/auth.ts" | `Path::exists()` |
| File deleted | "deleted old.rs" | `!Path::exists()` |
| File modified | "modified config.toml" | `git diff` against baseline |
| Package installed | "installed express" | Lockfile scan (up to 2 levels deep) |
| Test results | "all tests pass" | Structured runner output in transcript |
| Bug fix | "fixed the bug in src/parser.rs" | `git diff` for the referenced file |
| Numeric (files) | "edited 3 files" | `git diff --stat` file count |

Supported lockfiles: `package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`, `Cargo.lock`, `go.sum`, `Gemfile.lock`, `poetry.lock`.

### UNVERIFIABLE

A claim is UNVERIFIABLE when there isn't enough information to confirm or deny it:

- File modification claim in a non-git directory
- Package claim with no lockfile present
- Bug fix claim with no file path ("fixed the null pointer bug")
- Test claim with no structured runner output in the transcript
- Numeric claim for functions or lines (can't be derived from git)

---

## Example output

```
Truth score: 67%
[File] created src/auth.ts → PASS
[Package] installed jsonwebtoken → FAIL
  Reason: package not found in any lockfile
[Test] all tests pass → UNVERIFIABLE
  Reason: no test runner output found in transcript
Summary: 2 passed, 1 failed, 1 unverifiable
```

### JSON output

```json
{
  "truth_score": "67%",
  "summary": {
    "total": 4,
    "pass": 2,
    "fail": 1,
    "unverifiable": 1
  },
  "claims": [
    {
      "claim_type": "File",
      "raw_text": "created src/auth.ts",
      "result": "PASS",
      "reason": null
    },
    {
      "claim_type": "Package",
      "raw_text": "installed jsonwebtoken",
      "result": "FAIL",
      "reason": "package not found in any lockfile"
    }
  ]
}
```

---

## Truth score

```
score = PASS / (PASS + FAIL) × 100
```

UNVERIFIABLE claims are excluded from both numerator and denominator. If all claims are UNVERIFIABLE the score is reported as `N/A`. If no claims are extracted the output is `No verifiable claims found`.

---

## Session window

By default claimcheck checks uncommitted changes against `HEAD`. Use `--baseline` to widen the window:

```sh
# Everything since the main branch
claimcheck session.jsonl --baseline main

# Last 3 commits
claimcheck session.jsonl --baseline HEAD~3

# A specific commit
claimcheck session.jsonl --baseline a3f9c12
```

Uncommitted working-tree changes are always included regardless of baseline.

---

## Test verification

Test claims are verified against structured runner output found in the transcript itself — not against the claim's own wording. A claim saying "all tests pass" only verifies as PASS if the transcript also contains output like:

```
test result: ok. 23 passed; 0 failed
exit code: 0
```

If the runner output contradicts the claim ("all tests pass" but the transcript shows `5 failed`), the claim is marked FAIL.

---

## Limitations

- `--retest` auto-detects the test command from project files. Use `--test-cmd` to override.
- Numeric claims for function counts and line counts are always UNVERIFIABLE — git doesn't expose these directly.
- Bug fix claims without an explicit file path in the description are UNVERIFIABLE.
- Extraction is regex-based; unusual phrasing may not be caught.

---

## Development

```sh
# Run all tests
cargo test

# Run integration tests only
cargo test --test integration

# Run with a fixture
cargo run -- tests/fixtures/pass.jsonl --project-dir .
```

Tests live in `src/` (unit + property-based) and `tests/integration.rs` (end-to-end against real files).

---

## License

MIT — see [LICENSE](LICENSE).

---

## Repository

[github.com/ojuschugh1/claimcheck](https://github.com/ojuschugh1/claimcheck)
