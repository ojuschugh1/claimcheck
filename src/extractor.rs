use regex::Regex;
use std::sync::OnceLock;

use crate::types::{AssistantMessage, Claim, ClaimType, FileOp, NumericMetric};

struct FilePatterns {
    created: Regex,
    wrote: Regex,
    deleted: Regex,
    modified: Regex,
    updated: Regex,
}

struct PackagePatterns {
    installed: Regex,
    added: Regex,
    npm_install: Regex,
    cargo_add: Regex,
}

struct TestPatterns {
    all_tests_pass: Regex,
    ran_n_tests: Regex,
    tests_passed: Regex,
    n_tests_pass: Regex,
}

struct BugFixPatterns {
    fixed: Regex,
    resolved: Regex,
    patched: Regex,
}

struct NumericPatterns {
    edited_files: Regex,
    added_functions: Regex,
    changed_lines: Regex,
}

static FILE_PATTERNS: OnceLock<FilePatterns> = OnceLock::new();
static PACKAGE_PATTERNS: OnceLock<PackagePatterns> = OnceLock::new();
static TEST_PATTERNS: OnceLock<TestPatterns> = OnceLock::new();
static BUGFIX_PATTERNS: OnceLock<BugFixPatterns> = OnceLock::new();
static NUMERIC_PATTERNS: OnceLock<NumericPatterns> = OnceLock::new();
static PATH_RE: OnceLock<Regex> = OnceLock::new();

fn path_regex() -> &'static Regex {
    PATH_RE.get_or_init(|| Regex::new(r"\S*[/\.]\S+").unwrap())
}

fn file_patterns() -> &'static FilePatterns {
    FILE_PATTERNS.get_or_init(|| FilePatterns {
        created: Regex::new(r"(?i)\bcreated\s+(.+)").unwrap(),
        wrote: Regex::new(r"(?i)\bwrote\s+(.+)").unwrap(),
        deleted: Regex::new(r"(?i)\bdeleted\s+(.+)").unwrap(),
        modified: Regex::new(r"(?i)\bmodified\s+(.+)").unwrap(),
        updated: Regex::new(r"(?i)\bupdated\s+(.+)").unwrap(),
    })
}

fn package_patterns() -> &'static PackagePatterns {
    PACKAGE_PATTERNS.get_or_init(|| PackagePatterns {
        installed: Regex::new(r"(?i)installed\s+(\S+)").unwrap(),
        added: Regex::new(r"(?i)\badded\s+([^\d\s][^\s]{1,})").unwrap(),
        npm_install: Regex::new(r"(?i)npm install\s+(\S+)").unwrap(),
        cargo_add: Regex::new(r"(?i)cargo add\s+(\S+)").unwrap(),
    })
}

fn test_patterns() -> &'static TestPatterns {
    TEST_PATTERNS.get_or_init(|| TestPatterns {
        all_tests_pass: Regex::new(r"(?i)all\s+tests?\s+pass(ed)?").unwrap(),
        ran_n_tests: Regex::new(r"(?i)ran\s+(\d+)\s+tests?").unwrap(),
        tests_passed: Regex::new(r"(?i)tests?\s+passed").unwrap(),
        n_tests_pass: Regex::new(r"(?i)(\d+)\s+tests?\s+pass(ed)?").unwrap(),
    })
}

fn bugfix_patterns() -> &'static BugFixPatterns {
    BUGFIX_PATTERNS.get_or_init(|| BugFixPatterns {
        fixed: Regex::new(r"(?i)\bfixed\s+(?:the\s+)?(.+)").unwrap(),
        resolved: Regex::new(r"(?i)\bresolved\s+(?:the\s+)?(.+)").unwrap(),
        patched: Regex::new(r"(?i)\bpatched\s+(.+)").unwrap(),
    })
}

fn numeric_patterns() -> &'static NumericPatterns {
    NUMERIC_PATTERNS.get_or_init(|| NumericPatterns {
        edited_files: Regex::new(r"(?i)edited\s+(\d+)\s+files?").unwrap(),
        added_functions: Regex::new(r"(?i)added\s+(\d+)\s+functions?").unwrap(),
        changed_lines: Regex::new(r"(?i)changed\s+(\d+)\s+lines?").unwrap(),
    })
}

fn find_path_in(text: &str) -> Option<String> {
    path_regex().find(text).map(|m| {
        m.as_str()
            .trim_end_matches([',', '.', ')', ';'])
            .to_string()
    })
}

struct RawMatch {
    offset: usize,
    claim: Claim,
}

const NOT_PACKAGES: &[&str] = &[
    "a", "an", "the", "some", "new", "more", "support", "handling", "it", "this", "that", "them",
    "these", "those",
];

fn extract_file_claims(content: &str) -> Vec<RawMatch> {
    let p = file_patterns();
    let mut matches = Vec::new();

    let verbs: &[(&Regex, FileOp)] = &[
        (&p.created, FileOp::Create),
        (&p.wrote, FileOp::Create),
        (&p.deleted, FileOp::Delete),
        (&p.modified, FileOp::Modify),
        (&p.updated, FileOp::Modify),
    ];

    for (re, op) in verbs {
        for cap in re.captures_iter(content) {
            let m = cap.get(0).unwrap();
            let after_verb = cap.get(1).map(|g| g.as_str()).unwrap_or("");
            let identifier = find_path_in(after_verb);
            if identifier.is_some() {
                matches.push(RawMatch {
                    offset: m.start(),
                    claim: Claim {
                        claim_type: ClaimType::File,
                        raw_text: m.as_str().to_string(),
                        identifier,
                        file_op: Some(op.clone()),
                        numeric_value: None,
                        numeric_metric: None,
                    },
                });
            }
        }
    }
    matches
}

fn extract_package_claims(content: &str) -> Vec<RawMatch> {
    let p = package_patterns();
    let mut matches = Vec::new();

    for re in [&p.installed, &p.added, &p.npm_install, &p.cargo_add] {
        for cap in re.captures_iter(content) {
            let m = cap.get(0).unwrap();
            let identifier = cap.get(1).map(|g| g.as_str().to_string());
            if let Some(ref id) = identifier {
                if NOT_PACKAGES.contains(&id.to_lowercase().as_str()) {
                    continue;
                }
            }
            matches.push(RawMatch {
                offset: m.start(),
                claim: Claim {
                    claim_type: ClaimType::Package,
                    raw_text: m.as_str().to_string(),
                    identifier,
                    file_op: None,
                    numeric_value: None,
                    numeric_metric: None,
                },
            });
        }
    }
    matches
}

fn extract_test_claims(content: &str) -> Vec<RawMatch> {
    let p = test_patterns();
    let mut matches = Vec::new();

    for cap in p.all_tests_pass.captures_iter(content) {
        let m = cap.get(0).unwrap();
        matches.push(RawMatch {
            offset: m.start(),
            claim: Claim {
                claim_type: ClaimType::Test,
                raw_text: m.as_str().to_string(),
                identifier: None,
                file_op: None,
                numeric_value: None,
                numeric_metric: None,
            },
        });
    }
    for cap in p.ran_n_tests.captures_iter(content) {
        let m = cap.get(0).unwrap();
        let n = cap.get(1).and_then(|g| g.as_str().parse::<u64>().ok());
        matches.push(RawMatch {
            offset: m.start(),
            claim: Claim {
                claim_type: ClaimType::Test,
                raw_text: m.as_str().to_string(),
                identifier: n.map(|v| v.to_string()),
                file_op: None,
                numeric_value: n,
                numeric_metric: None,
            },
        });
    }
    for cap in p.tests_passed.captures_iter(content) {
        let m = cap.get(0).unwrap();
        matches.push(RawMatch {
            offset: m.start(),
            claim: Claim {
                claim_type: ClaimType::Test,
                raw_text: m.as_str().to_string(),
                identifier: None,
                file_op: None,
                numeric_value: None,
                numeric_metric: None,
            },
        });
    }
    for cap in p.n_tests_pass.captures_iter(content) {
        let m = cap.get(0).unwrap();
        let n = cap.get(1).and_then(|g| g.as_str().parse::<u64>().ok());
        matches.push(RawMatch {
            offset: m.start(),
            claim: Claim {
                claim_type: ClaimType::Test,
                raw_text: m.as_str().to_string(),
                identifier: n.map(|v| v.to_string()),
                file_op: None,
                numeric_value: n,
                numeric_metric: None,
            },
        });
    }
    matches
}

fn extract_bugfix_claims(content: &str) -> Vec<RawMatch> {
    let p = bugfix_patterns();
    let mut matches = Vec::new();

    for re in [&p.fixed, &p.resolved, &p.patched] {
        for cap in re.captures_iter(content) {
            let m = cap.get(0).unwrap();
            let description = cap.get(1).map(|g| g.as_str().trim()).unwrap_or("");
            let identifier = find_path_in(description);
            matches.push(RawMatch {
                offset: m.start(),
                claim: Claim {
                    claim_type: ClaimType::BugFix,
                    raw_text: m.as_str().to_string(),
                    identifier,
                    file_op: None,
                    numeric_value: None,
                    numeric_metric: None,
                },
            });
        }
    }
    matches
}

fn extract_numeric_claims(content: &str) -> Vec<RawMatch> {
    let p = numeric_patterns();
    let mut matches = Vec::new();

    let patterns: &[(&Regex, NumericMetric)] = &[
        (&p.edited_files, NumericMetric::FilesEdited),
        (&p.added_functions, NumericMetric::Functions),
        (&p.changed_lines, NumericMetric::Lines),
    ];

    for (re, metric) in patterns {
        for cap in re.captures_iter(content) {
            let m = cap.get(0).unwrap();
            let n = cap.get(1).and_then(|g| g.as_str().parse::<u64>().ok());
            matches.push(RawMatch {
                offset: m.start(),
                claim: Claim {
                    claim_type: ClaimType::Numeric,
                    raw_text: m.as_str().to_string(),
                    identifier: n.map(|v| v.to_string()),
                    file_op: None,
                    numeric_value: n,
                    numeric_metric: Some(metric.clone()),
                },
            });
        }
    }
    matches
}

pub fn extract_claims(messages: &[AssistantMessage]) -> Vec<Claim> {
    let mut all_claims = Vec::new();

    for msg in messages {
        let content = &msg.content;
        let mut raw: Vec<RawMatch> = Vec::new();
        raw.extend(extract_file_claims(content));
        raw.extend(extract_package_claims(content));
        raw.extend(extract_test_claims(content));
        raw.extend(extract_bugfix_claims(content));
        raw.extend(extract_numeric_claims(content));
        raw.sort_by_key(|m| m.offset);
        all_claims.extend(raw.into_iter().map(|rm| rm.claim));
    }

    all_claims
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AssistantMessage;
    use proptest::prelude::*;

    fn msg(content: &str) -> AssistantMessage {
        AssistantMessage {
            content: content.to_string(),
        }
    }

    #[test]
    fn file_created_direct_path() {
        let claims = extract_claims(&[msg("I created src/main.rs")]);
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].claim_type, ClaimType::File);
        assert_eq!(claims[0].identifier, Some("src/main.rs".to_string()));
        assert_eq!(claims[0].file_op, Some(FileOp::Create));
    }

    #[test]
    fn file_created_with_filler_words() {
        let claims = extract_claims(&[msg("created a new file src/auth.ts")]);
        let fc: Vec<_> = claims
            .iter()
            .filter(|c| c.claim_type == ClaimType::File)
            .collect();
        assert_eq!(fc.len(), 1);
        assert_eq!(fc[0].identifier, Some("src/auth.ts".to_string()));
    }

    #[test]
    fn file_created_bare_filename() {
        assert_eq!(
            extract_claims(&[msg("created foo.txt")])[0].identifier,
            Some("foo.txt".to_string())
        );
    }

    #[test]
    fn file_no_path_skipped() {
        let claims = extract_claims(&[msg("created a new module")]);
        assert!(claims.iter().all(|c| c.claim_type != ClaimType::File));
    }

    #[test]
    fn file_wrote() {
        assert_eq!(
            extract_claims(&[msg("wrote foo.txt")])[0].file_op,
            Some(FileOp::Create)
        );
    }

    #[test]
    fn file_deleted() {
        assert_eq!(
            extract_claims(&[msg("deleted old.rs")])[0].file_op,
            Some(FileOp::Delete)
        );
    }

    #[test]
    fn file_modified() {
        assert_eq!(
            extract_claims(&[msg("modified config.toml")])[0].file_op,
            Some(FileOp::Modify)
        );
    }

    #[test]
    fn file_updated() {
        assert_eq!(
            extract_claims(&[msg("updated README.md")])[0].file_op,
            Some(FileOp::Modify)
        );
    }

    #[test]
    fn package_installed() {
        let c = extract_claims(&[msg("installed serde")]);
        assert_eq!(c[0].claim_type, ClaimType::Package);
        assert_eq!(c[0].identifier, Some("serde".to_string()));
    }

    #[test]
    fn package_added() {
        assert_eq!(
            extract_claims(&[msg("added tokio")])[0].claim_type,
            ClaimType::Package
        );
    }

    #[test]
    fn added_numeric_not_package() {
        assert!(extract_claims(&[msg("added 3 functions")])
            .iter()
            .all(|c| c.claim_type != ClaimType::Package));
    }

    #[test]
    fn added_article_not_package() {
        assert!(extract_claims(&[msg("added a guard clause")])
            .iter()
            .all(|c| c.claim_type != ClaimType::Package));
    }

    #[test]
    fn npm_install() {
        let c = extract_claims(&[msg("npm install express")]);
        assert_eq!(c[0].claim_type, ClaimType::Package);
        assert_eq!(c[0].identifier, Some("express".to_string()));
    }

    #[test]
    fn cargo_add() {
        assert_eq!(
            extract_claims(&[msg("cargo add regex")])[0].claim_type,
            ClaimType::Package
        );
    }

    #[test]
    fn all_tests_pass() {
        assert_eq!(
            extract_claims(&[msg("all tests pass")])[0].claim_type,
            ClaimType::Test
        );
    }

    #[test]
    fn ran_n_tests() {
        assert_eq!(
            extract_claims(&[msg("ran 42 tests")])[0].numeric_value,
            Some(42)
        );
    }

    #[test]
    fn tests_passed() {
        assert_eq!(
            extract_claims(&[msg("tests passed")])[0].claim_type,
            ClaimType::Test
        );
    }

    #[test]
    fn n_tests_passed() {
        assert!(extract_claims(&[msg("5 tests passed")])
            .iter()
            .any(|c| c.claim_type == ClaimType::Test && c.numeric_value == Some(5)));
    }

    #[test]
    fn bugfix_with_file_path() {
        let c = extract_claims(&[msg("fixed the bug in src/parser.rs")]);
        assert_eq!(c[0].claim_type, ClaimType::BugFix);
        assert_eq!(c[0].identifier, Some("src/parser.rs".to_string()));
    }

    #[test]
    fn bugfix_without_file_path() {
        let c = extract_claims(&[msg("fixed the null pointer bug")]);
        assert_eq!(c[0].claim_type, ClaimType::BugFix);
        assert_eq!(c[0].identifier, None);
    }

    #[test]
    fn bugfix_resolved() {
        assert_eq!(
            extract_claims(&[msg("resolved the memory leak")])[0].claim_type,
            ClaimType::BugFix
        );
    }

    #[test]
    fn bugfix_patched() {
        assert_eq!(
            extract_claims(&[msg("patched the overflow issue")])[0].claim_type,
            ClaimType::BugFix
        );
    }

    #[test]
    fn numeric_edited_files() {
        let c = extract_claims(&[msg("edited 3 files")]);
        assert_eq!(c[0].numeric_value, Some(3));
        assert_eq!(c[0].numeric_metric, Some(NumericMetric::FilesEdited));
    }

    #[test]
    fn numeric_added_functions() {
        let claims = extract_claims(&[msg("added 5 functions")]);
        let num: Vec<_> = claims
            .iter()
            .filter(|c| c.claim_type == ClaimType::Numeric)
            .collect();
        assert_eq!(num[0].numeric_metric, Some(NumericMetric::Functions));
    }

    #[test]
    fn numeric_changed_lines() {
        assert_eq!(
            extract_claims(&[msg("changed 100 lines")])[0].numeric_metric,
            Some(NumericMetric::Lines)
        );
    }

    #[test]
    fn multiple_claims_in_one_message() {
        let claims = extract_claims(&[msg("created foo.rs and deleted bar.rs")]);
        assert_eq!(
            claims
                .iter()
                .filter(|c| c.claim_type == ClaimType::File)
                .count(),
            2
        );
    }

    #[test]
    fn contradictory_claims_both_extracted() {
        let claims = extract_claims(&[msg("created foo.ts then deleted foo.ts")]);
        let fc: Vec<_> = claims
            .iter()
            .filter(|c| c.claim_type == ClaimType::File)
            .collect();
        assert_eq!(fc.len(), 2);
        assert!(fc.iter().any(|c| c.file_op == Some(FileOp::Create)));
        assert!(fc.iter().any(|c| c.file_op == Some(FileOp::Delete)));
    }

    #[test]
    fn cursor_composer_format() {
        // Cursor {"type":"assistant","text":"..."} is parsed by parser, arrives as plain content
        let claims = extract_claims(&[msg("I created src/app.ts and installed axios")]);
        assert!(claims.iter().any(|c| c.claim_type == ClaimType::File));
        assert!(claims.iter().any(|c| c.claim_type == ClaimType::Package));
    }

    proptest! {
        #[test]
        fn prop_file_extraction(path in "[a-z][a-z0-9_/]{0,10}\\.[a-z]{1,4}") {
            let claims = extract_claims(&[msg(&format!("created {}", path))]);
            let fc: Vec<_> = claims.iter().filter(|c| c.claim_type == ClaimType::File).collect();
            prop_assert!(!fc.is_empty());
            prop_assert!(fc.iter().any(|c| c.identifier.as_deref() == Some(path.as_str())));
        }

        #[test]
        fn prop_package_extraction(pkg in "[a-z]{2}[a-z0-9_-]{0,20}") {
            let claims = extract_claims(&[msg(&format!("installed {}", pkg))]);
            let pc: Vec<_> = claims.iter().filter(|c| c.claim_type == ClaimType::Package).collect();
            prop_assert!(!pc.is_empty());
            prop_assert!(pc.iter().any(|c| c.identifier.as_deref() == Some(pkg.as_str())));
        }

        #[test]
        fn prop_test_extraction(n in 1u64..=1000u64) {
            let claims = extract_claims(&[msg(&format!("ran {} tests", n))]);
            let tc: Vec<_> = claims.iter().filter(|c| c.claim_type == ClaimType::Test).collect();
            prop_assert!(!tc.is_empty());
            prop_assert!(tc.iter().any(|c| c.identifier.as_deref() == Some(n.to_string().as_str())));
        }

        #[test]
        fn prop_numeric_extraction(n in 1u64..=100u64) {
            let claims = extract_claims(&[msg(&format!("edited {} files", n))]);
            let nc: Vec<_> = claims.iter().filter(|c| c.claim_type == ClaimType::Numeric).collect();
            prop_assert!(!nc.is_empty());
            prop_assert!(nc.iter().any(|c| c.identifier.as_deref() == Some(n.to_string().as_str())));
        }
    }
}
