use crate::ast::{Block, Claim, DefDecl, Document, ExpectFailType, Proof, Span};
use crate::dim::{collect_units, DimOutcome};
use verso_symbolic::{subscript_base, Context, Expr, ExprKind};

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
            Outcome::Pass
                | Outcome::NumericalPass { .. }
                | Outcome::ProofPass { .. }
                | Outcome::ExpectFailPass
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
    /// A def whose RHS has a dimensional error.
    DefDimError {
        error: String,
    },
    /// expect_fail block: inner verification had at least one failure (test passes).
    ExpectFailPass,
    /// expect_fail block: no inner check failed with the expected type.
    ExpectFailFail {
        /// What actually happened with each inner result.
        actual: Vec<String>,
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
                if let Some(ref dim) = decl.dimension {
                    ctx.declare_var(&decl.name, Some(dim.clone()));
                }
                if let Some(result) = check_def_dim(decl, &ctx) {
                    results.push(result);
                }
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
            Block::ExpectFail {
                name,
                failure_type,
                blocks,
                span,
            } => {
                results.push(verify_expect_fail(name, failure_type, blocks, span, &ctx));
            }
            _ => {}
        }
    }

    VerificationReport { results }
}

/// Verify an expect_fail block. Runs the inner blocks in isolation.
/// Succeeds only when at least one inner result fails with the specified type.
fn verify_expect_fail(
    name: &str,
    failure_type: &ExpectFailType,
    inner_blocks: &[Block],
    span: &Span,
    parent_ctx: &Context,
) -> VerificationResult {
    let inner_report = verify_blocks(inner_blocks, parent_ctx);
    let has_expected_failure = inner_report.results.iter().any(|r| {
        match failure_type {
            ExpectFailType::Symbolic => {
                matches!(r.outcome, Outcome::Fail { .. } | Outcome::ProofStepFail { .. })
            }
            ExpectFailType::DimensionMismatch => {
                matches!(r.dim_outcome, Some(DimOutcome::LhsRhsMismatch { .. }))
            }
            ExpectFailType::DimensionError => {
                matches!(r.dim_outcome, Some(DimOutcome::ExprError { .. }))
                    || matches!(r.outcome, Outcome::DefDimError { .. })
            }
        }
    });
    let outcome = if has_expected_failure {
        Outcome::ExpectFailPass
    } else {
        Outcome::ExpectFailFail {
            actual: inner_report.results.iter().map(describe_result).collect(),
        }
    };
    VerificationResult {
        name: name.to_string(),
        span: *span,
        outcome,
        dim_outcome: None,
        units: vec![],
    }
}

/// Summarize a verification result for diagnostic messages.
fn describe_result(r: &VerificationResult) -> String {
    let mut parts = Vec::new();
    match &r.outcome {
        Outcome::Pass => parts.push(format!("'{}' passed symbolically", r.name)),
        Outcome::NumericalPass { samples, .. } => {
            parts.push(format!("'{}' passed numerically ({} samples)", r.name, samples))
        }
        Outcome::Fail { residual } => {
            parts.push(format!("'{}' failed symbolically (residual: {})", r.name, residual))
        }
        Outcome::ProofPass { steps } => {
            parts.push(format!("'{}' proof passed ({} steps)", r.name, steps))
        }
        Outcome::ProofStepFail { step_index, .. } => {
            parts.push(format!("'{}' proof step {} failed", r.name, step_index))
        }
        Outcome::DefDimError { error } => {
            parts.push(format!("def '{}' dimension error: {}", r.name, error))
        }
        Outcome::ExpectFailPass | Outcome::ExpectFailFail { .. } => {}
    }
    if let Some(ref dim) = r.dim_outcome {
        match dim {
            DimOutcome::Pass => parts.push("dimensions ok".into()),
            DimOutcome::Skipped { .. } => {}
            DimOutcome::LhsRhsMismatch { lhs, rhs } => {
                parts.push(format!("dimension mismatch: lhs {}, rhs {}", lhs, rhs))
            }
            DimOutcome::ExprError { side, error } => {
                parts.push(format!("dimension error in {}: {}", side, error))
            }
        }
    }
    parts.join("; ")
}

/// Verify a slice of blocks, building on a parent context's declarations.
/// Creates a fresh context and re-registers the parent's consts, vars, and funcs.
fn verify_blocks(blocks: &[Block], parent_ctx: &Context) -> VerificationReport {
    let mut ctx = Context::new();
    // Inherit parent declarations
    for (name, expr) in &parent_ctx.consts {
        ctx.declare_const(name, expr.clone());
    }
    for (name, func) in &parent_ctx.funcs {
        ctx.declare_func(name, func.params.clone(), func.body.clone());
    }
    ctx.dims = parent_ctx.dims.clone();

    let mut results = Vec::new();
    for block in blocks {
        match block {
            Block::Var(decl) => {
                ctx.declare_var(&decl.var_name, Some(decl.dimension.clone()));
            }
            Block::Def(decl) => {
                ctx.declare_const(&decl.name, decl.value.clone());
                if let Some(ref dim) = decl.dimension {
                    ctx.declare_var(&decl.name, Some(dim.clone()));
                }
                if let Some(result) = check_def_dim(decl, &ctx) {
                    results.push(result);
                }
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
            Block::ExpectFail {
                name,
                failure_type,
                blocks: inner,
                span,
            } => {
                results.push(verify_expect_fail(name, failure_type, inner, span, &ctx));
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

/// Check dimensional consistency of a def's RHS expression.
/// Returns a failing VerificationResult if the RHS has a dim error or if
/// the RHS dimension doesn't match the declared dimension for the def's
/// base name (e.g. `var c_{mode} [L T^-1]` constrains all `c_{*}` defs).
fn check_def_dim(decl: &DefDecl, ctx: &Context) -> Option<VerificationResult> {
    match ctx.check_expr_dim(&decl.value) {
        Some(Err(e)) => Some(VerificationResult {
            name: decl.name.clone(),
            span: decl.span,
            outcome: Outcome::DefDimError {
                error: e.to_string(),
            },
            dim_outcome: None,
            units: vec![],
        }),
        Some(Ok(rhs_dim)) => {
            // Check that the RHS dimension matches the declared base-name dimension
            let base = subscript_base(&decl.name);
            if let Some(declared) = ctx.dims.get(base) {
                if !rhs_dim.is_dimensionless() && declared != &rhs_dim {
                    return Some(VerificationResult {
                        name: decl.name.clone(),
                        span: decl.span,
                        outcome: Outcome::DefDimError {
                            error: format!(
                                "expected {} (from {}), got {}",
                                declared, base, rhs_dim
                            ),
                        },
                        dim_outcome: None,
                        units: vec![],
                    });
                }
            }
            None
        }
        _ => None,
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
    fn verify_subscripted_def_substitution() {
        let src = "\
var c_{s} [L T^-1]
def c_{t} := c_{s}
claim torsion_matches_shear
  c_{t} = c_{s}
";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        assert!(
            report.all_passed(),
            "subscripted def substitution should pass symbolically: {:?}",
            report.results
        );
        // Must be symbolic, not just numerical
        assert!(
            matches!(report.results[0].outcome, Outcome::Pass),
            "expected symbolic Pass, got {:?}",
            report.results[0].outcome
        );
    }

    #[test]
    fn verify_chained_subscripted_def() {
        // c_{t} := c_{s} := sqrt(μ / ρ_{0}), both should resolve fully
        let src = "\
var μ [M L^-1 T^-2]
var ρ_{0} [M L^-3]
def c_{s} := sqrt(μ / ρ_{0})
def c_{t} := c_{s}
claim torsion_matches_shear
  c_{t} = c_{s}
";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        assert!(
            report.all_passed(),
            "chained subscripted def should pass symbolically: {:?}",
            report.results
        );
        assert!(
            matches!(report.results[0].outcome, Outcome::Pass),
            "expected symbolic Pass, got {:?}",
            report.results[0].outcome
        );
    }

    #[test]
    fn verify_subscripted_def_in_expression() {
        // def with subscripted name used in a more complex expression
        let src = "\
var P_{in} [M L^-1 T^-2]
var P_{el} [M L^-1 T^-2]
def L := P_{in} / P_{el}
claim roundtrip
  L * P_{el} = P_{in}
";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        assert!(
            report.all_passed(),
            "subscripted def in expression should pass: {:?}",
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
    fn verify_def_lhs_dim_registers_dimension() {
        // def with LHS dimension should make the name participate in dim checks
        let src = "\
def c_{s} [L T^-1] := 5
claim speed
  c_{s} + c_{s} = 2 * c_{s}
";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        let claim = report.results.iter().find(|r| r.name == "speed").unwrap();
        assert!(claim.passed(), "claim should pass: {:?}", claim);
        assert!(
            claim.dim_outcome.is_some(),
            "should have dim_outcome because def declared [L T^-1]"
        );
    }

    #[test]
    fn verify_def_lhs_dim_catches_mismatch() {
        // def with [L T^-1] used in addition with [M] should fail dim check
        let src = "\
var m [M]
def c_{s} [L T^-1] := 5
claim bad
  c_{s} = m
";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        let claim = report.results.iter().find(|r| r.name == "bad").unwrap();
        assert!(!claim.passed(), "adding velocity to mass should fail dim check");
    }

    #[test]
    fn verify_def_rhs_dim_must_match_base_var() {
        // var c_{mode} declares base "c" as [L T^-1].
        // def c_{p} := expr should error if expr has a different dimension.
        let src = "\
var ρ_{0} [M L^-3]
var K [M L^-1 T^-2]
var c_{mode} [L^2 T^-1]
def c_{p} := sqrt(K / ρ_{0})
claim trivial
  K = K
";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        let def_result = report.results.iter().find(|r| r.name == "c_{p}");
        assert!(
            def_result.is_some(),
            "def c_{{p}} should produce a dim error result"
        );
        assert!(
            !def_result.unwrap().passed(),
            "def c_{{p}} RHS is [L T^-1] but base c is declared [L^2 T^-1]"
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
    fn verify_def_rhs_dim_error() {
        // def whose RHS has a dimension error should be reported
        let src = "\
var μ [M L^-1 T^-2]
var ρ_{0} [M L^-3]
def bad := sqrt(μ / ρ_{0}) + 1
claim trivial
  μ = μ
";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        // Should have 2 results: one for the def error, one for the claim
        let def_result = report.results.iter().find(|r| r.name == "bad");
        assert!(def_result.is_some(), "def dim error should produce a result");
        let def_result = def_result.unwrap();
        assert!(!def_result.passed(), "def with dim error should not pass");
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

    #[test]
    fn expect_fail_passes_on_dim_mismatch() {
        let src = "\
var v [L T^-1]
var a [L T^-2]

expect_fail wrong_dim [dimension_mismatch]
  claim bad
    v = a";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        assert_eq!(report.results.len(), 1);
        let result = &report.results[0];
        assert_eq!(result.name, "wrong_dim");
        assert!(
            result.passed(),
            "expect_fail should pass when inner claim fails with dim mismatch"
        );
        assert!(matches!(result.outcome, Outcome::ExpectFailPass));
    }

    #[test]
    fn expect_fail_fails_when_wrong_type() {
        // claim x = x passes symbolically, so expect_fail [symbolic] should fail
        let src = "\
expect_fail all_pass [symbolic]
  claim ok
    x = x";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        assert_eq!(report.results.len(), 1);
        let result = &report.results[0];
        assert_eq!(result.name, "all_pass");
        assert!(
            !result.passed(),
            "expect_fail symbolic should fail when inner claim passes"
        );
        assert!(matches!(result.outcome, Outcome::ExpectFailFail { .. }));
        // Should report what actually happened
        if let Outcome::ExpectFailFail { actual } = &result.outcome {
            assert!(!actual.is_empty(), "should describe what actually happened");
            assert!(
                actual[0].contains("passed symbolically"),
                "should say claim passed: {}",
                actual[0]
            );
        }
    }

    #[test]
    fn expect_fail_inherits_parent_context() {
        // The parent defines c; the inner block uses c in a claim that should fail
        let src = "\
def c := 3*10^8
var v [L T^-1]

expect_fail dim_mismatch_with_const [dimension_mismatch]
  claim bad
    c = v";
        let doc = parse_document(src).unwrap();
        let report = verify_document(&doc);
        assert_eq!(report.results.len(), 1);
        assert!(
            report.results[0].passed(),
            "should pass: c (dimensionless) != v (L T^-1)"
        );
    }
}
