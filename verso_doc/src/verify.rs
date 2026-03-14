use crate::ast::{Block, Claim, Document, Proof, Span};
use crate::dim::{check_claim_dim, collect_units, DimEnv, DimOutcome};
use crate::eval::spot_check;
use verso_symbolic::{Expr, RuleSet, simplify};

const SPOT_CHECK_SAMPLES: usize = 200;

#[derive(Debug)]
pub struct VerificationReport {
    pub results: Vec<VerificationResult>,
}

impl VerificationReport {
    pub fn pass_count(&self) -> usize {
        self.results.iter().filter(|r| r.passed()).count()
    }

    pub fn fail_count(&self) -> usize {
        self.results.iter().filter(|r| !r.passed()).count()
    }

    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.passed())
    }
}

#[derive(Debug)]
pub struct VerificationResult {
    pub name: String,
    pub span: Span,
    pub outcome: Outcome,
    /// Result of dimensional analysis (None if no :dim declarations in document).
    pub dim_outcome: Option<DimOutcome>,
    /// Unit annotations found in the claim expressions.
    pub units: Vec<String>,
}

impl VerificationResult {
    pub fn passed(&self) -> bool {
        let symbolic_pass = matches!(
            self.outcome,
            Outcome::Pass | Outcome::NumericalPass { .. } | Outcome::ProofPass { .. }
        );
        let dim_pass = self
            .dim_outcome
            .as_ref()
            .map_or(true, |d| d.passed());
        symbolic_pass && dim_pass
    }
}

#[derive(Debug)]
pub enum Outcome {
    Pass,
    /// Symbolic simplification failed, but numerical spot-checks passed.
    NumericalPass { samples: usize, residual: Expr },
    Fail { residual: Expr },
    ProofPass { steps: usize },
    ProofStepFail {
        step_index: usize,
        from: Expr,
        to: Expr,
        residual: Expr,
        step_span: Span,
    },
}

/// Verify all claims and proofs in a document.
pub fn verify_document(doc: &Document) -> VerificationReport {
    let rules = RuleSet::full();
    let mut results = Vec::new();

    // Collect dimension declarations
    let mut dim_env = DimEnv::new();
    for block in &doc.blocks {
        if let Block::Dim(decl) = block {
            dim_env.insert(decl.var_name.clone(), decl.dimension.clone());
        }
    }
    let has_dims = !dim_env.is_empty();

    for block in &doc.blocks {
        match block {
            Block::Claim(claim) => {
                results.push(verify_claim(claim, &rules, has_dims.then_some(&dim_env)));
            }
            Block::Proof(proof) => {
                results.push(verify_proof(proof, &rules));
            }
            _ => {}
        }
    }

    VerificationReport { results }
}

/// Verify a single claim by checking that `lhs - rhs` simplifies to 0.
fn verify_claim(
    claim: &Claim,
    rules: &RuleSet,
    dim_env: Option<&DimEnv>,
) -> VerificationResult {
    let outcome = check_equal(&claim.lhs, &claim.rhs, rules);
    let dim_outcome = dim_env.map(|env| check_claim_dim(&claim.lhs, &claim.rhs, env));
    let mut units = collect_units(&claim.lhs);
    units.extend(collect_units(&claim.rhs));
    units.sort();
    units.dedup();
    VerificationResult {
        name: claim.name.clone(),
        span: claim.span,
        outcome,
        dim_outcome,
        units,
    }
}

/// Verify a proof chain: each adjacent pair of steps must be equivalent.
fn verify_proof(proof: &Proof, rules: &RuleSet) -> VerificationResult {
    for i in 0..proof.steps.len() - 1 {
        let from = &proof.steps[i];
        let to = &proof.steps[i + 1];

        // If justification names a specific rule, try it first
        if let Some(ref rule_name) = to.justification {
            if let Some(rule) = rules.find_rule(rule_name) {
                // Try applying the named rule at any subexpression of `from`
                if try_rule_produces(&from.expr, rule, &to.expr, rules) {
                    continue;
                }
            }
            // If the named rule didn't work, fall through to general simplification
        }

        // General check: simplify(from - to) == 0
        let diff = Expr::Add(
            Box::new(from.expr.clone()),
            Box::new(Expr::Neg(Box::new(to.expr.clone()))),
        );
        let result = simplify(&diff, rules);

        if !is_zero(&result) {
            return VerificationResult {
                name: format!("proof:{}", proof.claim_name),
                span: proof.span,
                outcome: Outcome::ProofStepFail {
                    step_index: i + 1,
                    from: from.expr.clone(),
                    to: to.expr.clone(),
                    residual: result,
                    step_span: to.span,
                },
                dim_outcome: None,
                units: Vec::new(),
            };
        }
    }

    VerificationResult {
        name: format!("proof:{}", proof.claim_name),
        span: proof.span,
        outcome: Outcome::ProofPass {
            steps: proof.steps.len(),
        },
        dim_outcome: None,
        units: Vec::new(),
    }
}

/// Check if two expressions are equivalent.
/// First tries symbolic simplification. If that fails, falls back to numerical spot-checks.
fn check_equal(lhs: &Expr, rhs: &Expr, rules: &RuleSet) -> Outcome {
    let diff = Expr::Add(
        Box::new(lhs.clone()),
        Box::new(Expr::Neg(Box::new(rhs.clone()))),
    );
    let result = simplify(&diff, rules);

    if is_zero(&result) {
        return Outcome::Pass;
    }

    // Symbolic didn't reduce to 0 — try numerical spot-check
    match spot_check(lhs, rhs, SPOT_CHECK_SAMPLES) {
        Ok(()) => Outcome::NumericalPass {
            samples: SPOT_CHECK_SAMPLES,
            residual: result,
        },
        Err(_) => Outcome::Fail { residual: result },
    }
}

/// Try applying a named rule at every subexpression of `from` (both LTR and RTL)
/// and check if any result equals `to`.
fn try_rule_produces(from: &Expr, rule: &verso_symbolic::Rule, to: &Expr, rules: &RuleSet) -> bool {
    // Try at root
    if let Some(result) = rule.apply_ltr(from) {
        if exprs_equivalent(&result, to, rules) {
            return true;
        }
    }
    if rule.reversible {
        if let Some(result) = rule.apply_rtl(from) {
            if exprs_equivalent(&result, to, rules) {
                return true;
            }
        }
    }

    // Try at subexpressions
    match from {
        Expr::Add(a, b) | Expr::Mul(a, b) | Expr::Pow(a, b) => {
            try_rule_produces(a, rule, to, rules) || try_rule_produces(b, rule, to, rules)
        }
        Expr::Neg(inner) | Expr::Inv(inner) | Expr::Fn(_, inner) => {
            try_rule_produces(inner, rule, to, rules)
        }
        Expr::FnN(_, args) => args.iter().any(|a| try_rule_produces(a, rule, to, rules)),
        _ => false,
    }
}

/// Check if two expressions are equivalent via simplification.
fn exprs_equivalent(a: &Expr, b: &Expr, rules: &RuleSet) -> bool {
    if a == b {
        return true;
    }
    let diff = Expr::Add(
        Box::new(a.clone()),
        Box::new(Expr::Neg(Box::new(b.clone()))),
    );
    is_zero(&simplify(&diff, rules))
}

fn is_zero(expr: &Expr) -> bool {
    match expr {
        Expr::Rational(r) => r.is_zero(),
        Expr::FracPi(r) => r.is_zero(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_document;

    #[test]
    fn verify_trivial_identity() {
        let doc = parse_document(":claim trivial\n  x = x").unwrap();
        let report = verify_document(&doc);
        assert_eq!(report.pass_count(), 1);
        assert_eq!(report.fail_count(), 0);
    }

    #[test]
    fn verify_add_zero() {
        let doc = parse_document(":claim add_zero\n  x + 0 = x").unwrap();
        let report = verify_document(&doc);
        assert!(report.all_passed());
    }

    #[test]
    fn verify_failing_claim() {
        let doc = parse_document(":claim wrong\n  x + 1 = x").unwrap();
        let report = verify_document(&doc);
        assert_eq!(report.fail_count(), 1);
    }

    #[test]
    fn verify_pythagorean() {
        let doc =
            parse_document(":claim pythag\n  sin(x)^2 + cos(x)^2 = 1").unwrap();
        let report = verify_document(&doc);
        assert!(report.all_passed(), "pythagorean identity should pass");
    }

    #[test]
    fn verify_multiple_claims() {
        let src = "\
:claim id1
  x + 0 = x

:claim id2
  x * 1 = x

:claim bad
  x + 1 = x
";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        assert_eq!(report.pass_count(), 2);
        assert_eq!(report.fail_count(), 1);
    }

    #[test]
    fn verify_simple_proof() {
        let src = "\
:proof identity
  x + 0
  = x
";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        assert!(report.all_passed());
        match &report.results[0].outcome {
            Outcome::ProofPass { steps } => assert_eq!(*steps, 2),
            other => panic!("expected ProofPass, got {:?}", other),
        }
    }

    #[test]
    fn verify_multi_step_proof() {
        let src = "\
:proof expand
  (x + 1)(x + 1)
  = x^2 + x + x + 1
  = x^2 + 2x + 1
";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        assert!(
            report.all_passed(),
            "multi-step proof should pass: {:?}",
            report.results
        );
    }

    #[test]
    fn verify_claim_collects_units() {
        let src = ":claim unit_conv\n  1000 [m] = 1 [km]";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        assert_eq!(report.results[0].units, vec!["km", "m"]);
    }

    #[test]
    fn verify_claim_no_units_is_empty() {
        let doc = parse_document(":claim trivial\n  x = x").unwrap();
        let report = verify_document(&doc);
        assert!(report.results[0].units.is_empty());
    }

    #[test]
    fn verify_proof_with_bad_step() {
        let src = "\
:proof bad
  x
  = x + 1
";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        assert_eq!(report.fail_count(), 1);
        match &report.results[0].outcome {
            Outcome::ProofStepFail { step_index, .. } => assert_eq!(*step_index, 1),
            other => panic!("expected ProofStepFail, got {:?}", other),
        }
    }
}
