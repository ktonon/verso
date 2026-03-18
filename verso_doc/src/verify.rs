use crate::ast::{Block, Claim, Document, Proof, Span};
use crate::dim::{collect_units, DimOutcome};
use verso_symbolic::{Context, Expr, ExprKind};

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
    /// Result of dimensional analysis (None if no var declarations in document).
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
        let dim_pass = self.dim_outcome.as_ref().map_or(true, |d| d.passed());
        symbolic_pass && dim_pass
    }
}

#[derive(Debug)]
pub enum Outcome {
    Pass,
    /// Symbolic simplification failed, but numerical spot-checks passed.
    NumericalPass {
        samples: usize,
        residual: Expr,
    },
    Fail {
        residual: Expr,
    },
    ProofPass {
        steps: usize,
    },
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

    for block in &doc.blocks {
        match block {
            Block::Var(decl) => {
                ctx.declare_var(&decl.var_name, Some(decl.dimension.clone()));
            }
            Block::Def(decl) => {
                ctx.declare_const(&decl.name, decl.value.clone());
            }
            Block::Func(decl) => {
                ctx.declare_func(&decl.name, decl.params.clone(), decl.body.clone());
            }
            Block::Claim(claim) => {
                let result = verify_claim(claim, &ctx);
                if result.passed() {
                    ctx.add_claim_as_rule(&claim.name, &claim.lhs, &claim.rhs);
                }
                results.push(result);
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
        verso_symbolic::EqualityResult::NotEqual { residual } => Outcome::Fail { residual },
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
    // Check dimensions: first step vs last step (a proof asserts first = last)
    let dim_outcome = if ctx.has_dims() {
        let first = &proof.steps.first().unwrap().expr;
        let last = &proof.steps.last().unwrap().expr;
        Some(ctx.check_dims(first, last))
    } else {
        None
    };

    let mut units: Vec<String> = Vec::new();
    for step in &proof.steps {
        units.extend(collect_units(&step.expr));
    }
    units.sort();
    units.dedup();

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

        if !ctx.exprs_equivalent(&from.expr, &to.expr) {
            let diff = Expr::derived(ExprKind::Add(
                Box::new(from.expr.clone()),
                Box::new(Expr::derived(ExprKind::Neg(Box::new(to.expr.clone())))),
            ));
            let result = ctx.simplify(&diff);
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
                dim_outcome,
                units,
            };
        }
    }

    VerificationResult {
        name: format!("proof:{}", proof.claim_name),
        span: proof.span,
        outcome: Outcome::ProofPass {
            steps: proof.steps.len(),
        },
        dim_outcome,
        units,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::parse_document;

    #[test]
    fn verify_trivial_identity() {
        let doc = parse_document("claim trivial\n  x = x").unwrap();
        let report = verify_document(&doc);
        assert_eq!(report.pass_count(), 1);
        assert_eq!(report.fail_count(), 0);
    }

    #[test]
    fn verify_add_zero() {
        let doc = parse_document("claim add_zero\n  x + 0 = x").unwrap();
        let report = verify_document(&doc);
        assert!(report.all_passed());
    }

    #[test]
    fn verify_failing_claim() {
        let doc = parse_document("claim wrong\n  x + 1 = x").unwrap();
        let report = verify_document(&doc);
        assert_eq!(report.fail_count(), 1);
    }

    #[test]
    fn verify_pythagorean() {
        let doc = parse_document("claim pythag\n  sin(x)^2 + cos(x)^2 = 1").unwrap();
        let report = verify_document(&doc);
        assert!(report.all_passed(), "pythagorean identity should pass");
    }

    #[test]
    fn verify_multiple_claims() {
        let src = "\
claim id1
  x + 0 = x

claim id2
  x * 1 = x

claim bad
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
proof identity
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
proof expand
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
        let src = "claim unit_conv\n  1000 [m] = 1 [km]";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        assert_eq!(report.results[0].units, vec!["km", "m"]);
    }

    #[test]
    fn verify_claim_no_units_is_empty() {
        let doc = parse_document("claim trivial\n  x = x").unwrap();
        let report = verify_document(&doc);
        assert!(report.results[0].units.is_empty());
    }

    #[test]
    fn verify_proof_with_bad_step() {
        let src = "\
proof bad
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

    #[test]
    fn verify_const_substitution() {
        let src = "\
def k := 2
claim double
  k * x = 2 * x
";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        assert!(
            report.all_passed(),
            "const substitution should pass: {:?}",
            report.results
        );
    }

    #[test]
    fn verify_const_in_proof() {
        let src = "\
def a := 3
proof expand
  a * (x + 1)
  = 3 * x + 3
";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        assert!(
            report.all_passed(),
            "const in proof should pass: {:?}",
            report.results
        );
    }

    #[test]
    fn verify_func_expansion() {
        let src = "\
func sq(x) := x^2
claim square
  sq(3) = 9
";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        assert!(
            report.all_passed(),
            "func expansion should pass: {:?}",
            report.results
        );
    }

    #[test]
    fn verify_func_two_params() {
        let src = "\
func KE(m, v) := (1/2) * m * v^2
claim energy
  KE(2, 3) = 9
";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        assert!(
            report.all_passed(),
            "multi-param func should pass: {:?}",
            report.results
        );
    }

    #[test]
    fn verify_func_with_const() {
        let src = "\
def g := 10
func PE(m, h) := m * g * h
claim potential
  PE(2, 5) = 100
";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        assert!(
            report.all_passed(),
            "func with const should pass: {:?}",
            report.results
        );
    }

    #[test]
    fn verify_claim_becomes_rule() {
        // First claim establishes a rule; second claim uses it
        let src = "\
claim double
  2 * x = x + x

claim quadruple
  4 * x = 2 * x + 2 * x
";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        assert!(
            report.all_passed(),
            "claims-as-rules should pass: {:?}",
            report.results
        );
    }

    #[test]
    fn verify_def_rhs_only_var_no_panic() {
        // Regression: a def where σ appears only in the RHS must not panic
        let src = "\
def a := b / σ

claim trivial
  x = x
";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        assert!(
            report.all_passed(),
            "rhs-only var should not panic: {:?}",
            report.results
        );
    }

    #[test]
    fn verify_const_wrong_value_fails() {
        let src = "\
def k := 2
claim wrong
  k * x = 3 * x
";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        assert_eq!(report.fail_count(), 1);
    }

    #[test]
    fn verify_claim_with_const_catches_dim_error() {
        let src = "\
var v [L T^-1]
def c := 5
claim bad
  v = c
";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        let result = &report.results[0];
        assert!(!result.passed(), "velocity = dimensionless should fail");
        // dim_outcome should NOT be Skipped — c is a known const
        assert!(result.dim_outcome.is_some());
        match result.dim_outcome.as_ref().unwrap() {
            DimOutcome::Skipped { .. } => {
                panic!("should not skip dim check when const is known")
            }
            _ => {} // LhsRhsMismatch or ExprError is expected
        }
    }

    #[test]
    fn verify_proof_has_dim_outcome() {
        let src = "\
var x [L]
proof double
  2 * x
  = x + x
";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        let result = &report.results[0];
        assert!(
            result.dim_outcome.is_some(),
            "proof should have dim_outcome when vars declared"
        );
        assert!(
            result.dim_outcome.as_ref().unwrap().passed(),
            "dim should pass for consistent proof"
        );
    }

    #[test]
    fn verify_proof_dim_mismatch_fails() {
        let src = "\
var v [L T^-1]
var t [T]
proof dim_bad
  v * t
  = v + t
";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        let result = &report.results[0];
        assert!(!result.passed());
        assert!(
            result.dim_outcome.is_some(),
            "proof should have dim_outcome"
        );
    }
}
