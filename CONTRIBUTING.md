# Contributing

## Getting started

```sh
git clone https://github.com/ojuschugh1/claimcheck
cd claimcheck
cargo build
cargo test
```

Requires Rust 1.70+ and `git` in PATH.

## Project structure

```
src/
  main.rs       pipeline orchestration
  cli.rs        argument parsing (clap)
  parser.rs     JSONL + Markdown transcript parsing
  extractor.rs  regex-based claim extraction
  verifier.rs   filesystem / git / lockfile verification
  git.rs        git helpers (shells out to git)
  lockfile.rs   lockfile discovery and package search
  scorer.rs     truth score calculation
  reporter.rs   text and JSON report formatting
  types.rs      shared data types

tests/
  integration.rs  end-to-end tests against real files
  fixtures/
    pass.jsonl          claims that should all PASS
    fail.jsonl          claims that should all FAIL
    unverifiable.jsonl  claims that should all be UNVERIFIABLE
```

## Running tests

```sh
# All tests
cargo test

# Unit + property tests only
cargo test --lib

# Integration tests only
cargo test --test integration

# A specific test
cargo test contract_bugfix_no_path
```

## Adding a new claim type

1. Add a variant to `ClaimType` in `types.rs`
2. Add regex patterns in `extractor.rs`
3. Add an extraction function following the existing pattern
4. Add a verification branch in `verifier.rs`
5. Add unit tests in the relevant module
6. Add a contract test in `tests/integration.rs`

## Adding a new transcript format

1. Add a variant to `TranscriptFormat` in `parser.rs`
2. Extend `detect_format()` with the new extension
3. Implement a parser function
4. Add tests covering the new format's marker styles

## Extraction patterns

Patterns are compiled once via `OnceLock` and reused. Keep them case-insensitive (`(?i)`) and anchored with `\b` where appropriate to avoid partial matches.

When adding a new pattern, add a corresponding property test that generates random identifiers embedded in the pattern and asserts correct extraction.

## Test philosophy

- Unit tests: example-based, one behaviour per test, short names
- Property tests: use proptest to cover the full input space for pure functions
- Integration tests: run the binary against real fixture files, assert on stdout

Avoid testing implementation details. Test observable behaviour.

## Submitting changes

1. Make sure `cargo test` passes
2. Make sure `cargo clippy` has no warnings
3. Update `CHANGELOG.md` under `## Unreleased`
4. Open a pull request with a clear description of what changed and why
