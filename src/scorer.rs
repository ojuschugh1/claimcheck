use crate::types::{TruthScore, VerificationResult, VerifiedClaim};

pub fn calculate_score(results: &[VerifiedClaim]) -> TruthScore {
    if results.is_empty() {
        return TruthScore::NoClaims;
    }

    let pass = results
        .iter()
        .filter(|vc| vc.result == VerificationResult::Pass)
        .count();
    let fail = results
        .iter()
        .filter(|vc| matches!(vc.result, VerificationResult::Fail { .. }))
        .count();
    let verifiable = pass + fail;

    if verifiable == 0 {
        return TruthScore::NotApplicable;
    }

    let score = ((pass as f64 / verifiable as f64) * 100.0).round() as u8;
    TruthScore::Score(score)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Claim, ClaimType, VerificationResult, VerifiedClaim};
    use proptest::prelude::*;

    fn verified(result: VerificationResult) -> VerifiedClaim {
        VerifiedClaim {
            claim: Claim {
                claim_type: ClaimType::File,
                raw_text: String::new(),
                identifier: None,
                file_op: None,
                numeric_value: None,
                numeric_metric: None,
            },
            result,
        }
    }

    #[test]
    fn empty_is_no_claims() {
        assert_eq!(calculate_score(&[]), TruthScore::NoClaims);
    }

    #[test]
    fn all_unverifiable_is_na() {
        let r = vec![
            verified(VerificationResult::Unverifiable { reason: "x".into() }),
            verified(VerificationResult::Unverifiable { reason: "y".into() }),
        ];
        assert_eq!(calculate_score(&r), TruthScore::NotApplicable);
    }

    #[test]
    fn all_pass() {
        let r = vec![
            verified(VerificationResult::Pass),
            verified(VerificationResult::Pass),
        ];
        assert_eq!(calculate_score(&r), TruthScore::Score(100));
    }

    #[test]
    fn all_fail() {
        let r = vec![
            verified(VerificationResult::Fail {
                reason: "bad".into(),
            }),
            verified(VerificationResult::Fail {
                reason: "bad".into(),
            }),
        ];
        assert_eq!(calculate_score(&r), TruthScore::Score(0));
    }

    #[test]
    fn mixed_excludes_unverifiable() {
        let r = vec![
            verified(VerificationResult::Pass),
            verified(VerificationResult::Pass),
            verified(VerificationResult::Fail {
                reason: "bad".into(),
            }),
            verified(VerificationResult::Fail {
                reason: "bad".into(),
            }),
            verified(VerificationResult::Unverifiable {
                reason: "n/a".into(),
            }),
        ];
        assert_eq!(calculate_score(&r), TruthScore::Score(50));
    }

    #[test]
    fn rounds_to_nearest() {
        // 1/3 = 33.33... → 33
        let r = vec![
            verified(VerificationResult::Pass),
            verified(VerificationResult::Fail {
                reason: "bad".into(),
            }),
            verified(VerificationResult::Fail {
                reason: "bad".into(),
            }),
        ];
        assert_eq!(calculate_score(&r), TruthScore::Score(33));
    }

    proptest! {
        #[test]
        fn prop_score_formula(
            pass_count in 0usize..=50,
            fail_count in 0usize..=50,
            unverifiable_count in 0usize..=50,
        ) {
            let mut claims = Vec::new();
            claims.extend((0..pass_count).map(|_| verified(VerificationResult::Pass)));
            claims.extend((0..fail_count).map(|_| verified(VerificationResult::Fail { reason: "x".into() })));
            claims.extend((0..unverifiable_count).map(|_| verified(VerificationResult::Unverifiable { reason: "x".into() })));

            let total = pass_count + fail_count + unverifiable_count;
            let verifiable = pass_count + fail_count;
            let result = calculate_score(&claims);

            if total == 0 {
                prop_assert_eq!(result, TruthScore::NoClaims);
            } else if verifiable == 0 {
                prop_assert_eq!(result, TruthScore::NotApplicable);
            } else {
                let expected = ((pass_count as f64 / verifiable as f64) * 100.0).round() as u8;
                prop_assert_eq!(result, TruthScore::Score(expected));
            }
        }
    }
}
