use crate::ast::{Block, Claim, Document, Proof, Span};
use crate::dim::{collect_units, DimOutcome};
use verso_symbolic::{Context, Expr, is_zero};

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
    /// Result of dimensional analysis (None if no :var declarations in document).
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
    let mut ctx = Context::new();
    let mut results = Vec::new();

    // Collect dimension declarations
    for block in &doc.blocks {
        if let Block::Var(decl) = block {
            ctx.declare_var(&decl.var_name, Some(decl.dimension.clone()));
        }
    }

    for block in &doc.blocks {
        match block {
            Block::Claim(claim) => {
                results.push(verify_claim(claim, &ctx));
            }
            Block::Proof(proof) => {
                results.push(verify_proof(proof, &ctx));
            }
            _ => {}
        }
    }

    VerificationReport { results }
}

/// Verify a single claim by checking that `lhs - rhs` simplifies to 0.
fn verify_claim(claim: &Claim, ctx: &Context) -> VerificationResult {
    let outcome = match ctx.check_equal(&claim.lhs, &claim.rhs) {
        verso_symbolic::EqualityResult::Equal => Outcome::Pass,
        verso_symbolic::EqualityResult::NumericallyEqual { samples, residual } => {
            Outcome::NumericalPass { samples, residual }
        }
        verso_symbolic::EqualityResult::NotEqual { residual } => {
            Outcome::Fail { residual }
        }
    };
    let dim_outcome = if ctx.has_dims() {
        Some(ctx.check_dims(&claim.lhs, &claim.rhs))
    } else {
        None
    };
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
fn verify_proof(proof: &Proof, ctx: &Context) -> VerificationResult {
    for i in 0..proof.steps.len() - 1 {
        let from = &proof.steps[i];
        let to = &proof.steps[i + 1];

        // If justification names a specific rule, try it first
        if let Some(ref rule_name) = to.justification {
            if let Some(rule) = ctx.rules.find_rule(rule_name) {
                if ctx.try_rule_produces(&from.expr, rule, &to.expr) {
                    continue;
                }
            }
        }

        // General check: simplify(from - to) == 0
        let diff = Expr::Add(
            Box::new(from.expr.clone()),
            Box::new(Expr::Neg(Box::new(to.expr.clone()))),
        );
        let result = ctx.simplify(&diff);

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
