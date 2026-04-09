use crate::types::{
    Claim, ClaimDetail, ClaimSummary, Report, TruthScore, VerificationResult, VerifiedClaim,
};

fn score_to_string(score: &TruthScore) -> String {
    match score {
        TruthScore::Score(n) => format!("{}%", n),
        TruthScore::NotApplicable => "N/A".to_string(),
        TruthScore::NoClaims => "N/A (no claims)".to_string(),
    }
}

pub fn format_text_report(score: &TruthScore, results: &[VerifiedClaim]) -> String {
    let mut lines: Vec<String> = Vec::new();

    if results.is_empty() {
        lines.push("No verifiable claims found".to_string());
    } else {
        match score {
            TruthScore::NoClaims => lines.push("No verifiable claims found".to_string()),
            TruthScore::NotApplicable => lines.push("Truth score: N/A".to_string()),
            TruthScore::Score(n) => lines.push(format!("Truth score: {}%", n)),
        }
    }

    for vc in results {
        let label = match &vc.result {
            VerificationResult::Pass => "PASS",
            VerificationResult::Fail { .. } => "FAIL",
            VerificationResult::Unverifiable { .. } => "UNVERIFIABLE",
        };
        lines.push(format!(
            "[{}] {} \u{2192} {}",
            vc.claim.claim_type, vc.claim.raw_text, label
        ));
        if let VerificationResult::Fail { reason } = &vc.result {
            lines.push(format!("  Reason: {}", reason));
        }
    }

    if !results.is_empty() {
        let pass = results
            .iter()
            .filter(|vc| vc.result == VerificationResult::Pass)
            .count();
        let fail = results
            .iter()
            .filter(|vc| matches!(vc.result, VerificationResult::Fail { .. }))
            .count();
        let unverifiable = results
            .iter()
            .filter(|vc| matches!(vc.result, VerificationResult::Unverifiable { .. }))
            .count();
        lines.push(format!(
            "Summary: {} passed, {} failed, {} unverifiable",
            pass, fail, unverifiable
        ));
    }

    lines.join("\n")
}

pub fn format_json_report(score: &TruthScore, results: &[VerifiedClaim]) -> String {
    let pass = results
        .iter()
        .filter(|vc| vc.result == VerificationResult::Pass)
        .count();
    let fail = results
        .iter()
        .filter(|vc| matches!(vc.result, VerificationResult::Fail { .. }))
        .count();
    let unverifiable = results
        .iter()
        .filter(|vc| matches!(vc.result, VerificationResult::Unverifiable { .. }))
        .count();

    let claims: Vec<ClaimDetail> = results
        .iter()
        .map(|vc| {
            let (result_str, reason) = match &vc.result {
                VerificationResult::Pass => ("PASS".to_string(), None),
                VerificationResult::Fail { reason } => ("FAIL".to_string(), Some(reason.clone())),
                VerificationResult::Unverifiable { reason } => {
                    ("UNVERIFIABLE".to_string(), Some(reason.clone()))
                }
            };
            ClaimDetail {
                claim_type: vc.claim.claim_type.to_string(),
                raw_text: vc.claim.raw_text.clone(),
                result: result_str,
                reason,
            }
        })
        .collect();

    let report = Report {
        truth_score: score_to_string(score),
        summary: ClaimSummary {
            total: results.len(),
            pass,
            fail,
            unverifiable,
        },
        claims,
    };

    serde_json::to_string_pretty(&report).unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
}

pub fn format_claims_list(claims: &[Claim]) -> String {
    claims
        .iter()
        .map(|c| match &c.identifier {
            Some(id) => format!("[{}] {} (identifier: {})", c.claim_type, c.raw_text, id),
            None => format!("[{}] {}", c.claim_type, c.raw_text),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Claim, ClaimType, VerificationResult, VerifiedClaim};
    use proptest::prelude::*;

    fn claim(raw_text: &str) -> Claim {
        Claim {
            claim_type: ClaimType::File,
            raw_text: raw_text.into(),
            identifier: None,
            file_op: None,
            numeric_value: None,
            numeric_metric: None,
        }
    }

    fn verified(raw_text: &str, result: VerificationResult) -> VerifiedClaim {
        VerifiedClaim {
            claim: claim(raw_text),
            result,
        }
    }

    #[test]
    fn empty_report() {
        assert_eq!(
            format_text_report(&TruthScore::NoClaims, &[]),
            "No verifiable claims found"
        );
    }

    #[test]
    fn text_report_pass_and_fail() {
        let results = vec![
            verified("created foo.rs", VerificationResult::Pass),
            verified(
                "created bar.rs",
                VerificationResult::Fail {
                    reason: "file not found".into(),
                },
            ),
        ];
        let report = format_text_report(&TruthScore::Score(50), &results);
        assert!(report.contains("Truth score: 50%"));
        assert!(report.contains("PASS"));
        assert!(report.contains("FAIL"));
        assert!(report.contains("Reason: file not found"));
        assert!(report.contains("Summary: 1 passed, 1 failed, 0 unverifiable"));
    }

    #[test]
    fn text_report_na() {
        let results = vec![verified(
            "created foo.rs",
            VerificationResult::Unverifiable {
                reason: "no git".into(),
            },
        )];
        let report = format_text_report(&TruthScore::NotApplicable, &results);
        assert!(report.contains("Truth score: N/A"));
    }

    #[test]
    fn json_report_valid() {
        let results = vec![verified("created foo.rs", VerificationResult::Pass)];
        let json = format_json_report(&TruthScore::Score(100), &results);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["truth_score"], "100%");
        assert_eq!(parsed["summary"]["pass"], 1);
    }

    #[test]
    fn claims_list_with_identifier() {
        let claims = vec![
            Claim {
                claim_type: ClaimType::File,
                raw_text: "created src/main.rs".into(),
                identifier: Some("src/main.rs".into()),
                file_op: None,
                numeric_value: None,
                numeric_metric: None,
            },
            Claim {
                claim_type: ClaimType::Package,
                raw_text: "installed serde".into(),
                identifier: None,
                file_op: None,
                numeric_value: None,
                numeric_metric: None,
            },
        ];
        let output = format_claims_list(&claims);
        assert!(output.contains("(identifier: src/main.rs)"));
        assert!(output.contains("[Package] installed serde"));
        assert!(!output.contains("identifier: )"));
    }

    #[test]
    fn score_strings() {
        assert_eq!(score_to_string(&TruthScore::Score(75)), "75%");
        assert_eq!(score_to_string(&TruthScore::NotApplicable), "N/A");
        assert_eq!(score_to_string(&TruthScore::NoClaims), "N/A (no claims)");
    }

    // Proptest strategies

    fn arb_string() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 _./:-]{1,40}"
            .prop_map(|s| s.trim().to_string())
            .prop_filter("non-empty", |s| !s.is_empty())
    }

    fn arb_claim_type() -> impl Strategy<Value = ClaimType> {
        prop_oneof![
            Just(ClaimType::File),
            Just(ClaimType::Package),
            Just(ClaimType::Test),
            Just(ClaimType::BugFix),
            Just(ClaimType::Numeric),
        ]
    }

    fn arb_claim() -> impl Strategy<Value = Claim> {
        (
            arb_claim_type(),
            arb_string(),
            proptest::option::of(arb_string()),
        )
            .prop_map(|(ct, rt, id)| Claim {
                claim_type: ct,
                raw_text: rt,
                identifier: id,
                file_op: None,
                numeric_value: None,
                numeric_metric: None,
            })
    }

    fn arb_result() -> impl Strategy<Value = VerificationResult> {
        prop_oneof![
            Just(VerificationResult::Pass),
            arb_string().prop_map(|r| VerificationResult::Fail { reason: r }),
            arb_string().prop_map(|r| VerificationResult::Unverifiable { reason: r }),
        ]
    }

    fn arb_verified() -> impl Strategy<Value = VerifiedClaim> {
        (arb_claim(), arb_result()).prop_map(|(c, r)| VerifiedClaim {
            claim: c,
            result: r,
        })
    }

    proptest! {
        #[test]
        fn prop_report_completeness(claims in proptest::collection::vec(arb_verified(), 1..=20)) {
            let pass = claims.iter().filter(|vc| vc.result == VerificationResult::Pass).count();
            let fail = claims.iter().filter(|vc| matches!(vc.result, VerificationResult::Fail { .. })).count();
            let unverifiable = claims.iter().filter(|vc| matches!(vc.result, VerificationResult::Unverifiable { .. })).count();

            let verifiable = pass + fail;
            let score = if verifiable == 0 { TruthScore::NotApplicable }
            else { TruthScore::Score(((pass as f64 / verifiable as f64) * 100.0).round() as u8) };

            let report = format_text_report(&score, &claims);

            let has_score = match &score {
                TruthScore::Score(n) => report.contains(&format!("Truth score: {}%", n)),
                TruthScore::NotApplicable => report.contains("Truth score: N/A"),
                TruthScore::NoClaims => report.contains("No verifiable claims found"),
            };
            prop_assert!(has_score);

            for vc in &claims {
                prop_assert!(report.contains(&vc.claim.raw_text));
                if let VerificationResult::Fail { reason } = &vc.result {
                    prop_assert!(report.contains(reason.as_str()));
                }
            }

            let summary = format!("Summary: {} passed, {} failed, {} unverifiable", pass, fail, unverifiable);
            prop_assert!(report.contains(&summary));
        }

        #[test]
        fn prop_json_round_trip(
            truth_score in arb_string(),
            total in 0usize..=50, pass in 0usize..=50,
            fail in 0usize..=50, unverifiable in 0usize..=50,
            claims in proptest::collection::vec(
                (arb_string(), arb_string(), arb_string(), proptest::option::of(arb_string())), 0..=10,
            ),
        ) {
            let details: Vec<ClaimDetail> = claims.into_iter()
                .map(|(ct, rt, res, reason)| ClaimDetail { claim_type: ct, raw_text: rt, result: res, reason })
                .collect();

            let original = Report {
                truth_score, summary: ClaimSummary { total, pass, fail, unverifiable }, claims: details,
            };

            let json = serde_json::to_string_pretty(&original).unwrap();
            let back: Report = serde_json::from_str(&json).unwrap();

            prop_assert_eq!(original.truth_score, back.truth_score);
            prop_assert_eq!(original.summary.total, back.summary.total);
            prop_assert_eq!(original.claims.len(), back.claims.len());
        }

        #[test]
        fn prop_claims_list_round_trip(claims in proptest::collection::vec(arb_claim(), 1..=10)) {
            let output = format_claims_list(&claims);
            let lines: Vec<&str> = output.lines().collect();
            prop_assert_eq!(lines.len(), claims.len());
            for (c, line) in claims.iter().zip(lines.iter()) {
                prop_assert!(line.contains(&c.raw_text));
            }
        }
    }
}
