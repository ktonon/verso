use crate::ast::{Block, Claim, Document, Span};
use erd_symbolic::{Expr, RuleSet, simplify};

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
}

impl VerificationResult {
    pub fn passed(&self) -> bool {
        matches!(self.outcome, Outcome::Pass)
    }
}

#[derive(Debug)]
pub enum Outcome {
    Pass,
    Fail { residual: Expr },
}

/// Verify all claims in a document.
pub fn verify_document(doc: &Document) -> VerificationReport {
    let rules = RuleSet::full();
    let mut results = Vec::new();

    for block in &doc.blocks {
        if let Block::Claim(claim) = block {
            results.push(verify_claim(claim, &rules));
        }
    }

    VerificationReport { results }
}

/// Verify a single claim by checking that `lhs - rhs` simplifies to 0.
fn verify_claim(claim: &Claim, rules: &RuleSet) -> VerificationResult {
    let diff = Expr::Add(
        Box::new(claim.lhs.clone()),
        Box::new(Expr::Neg(Box::new(claim.rhs.clone()))),
    );

    let result = simplify(&diff, rules);

    let outcome = if is_zero(&result) {
        Outcome::Pass
    } else {
        Outcome::Fail { residual: result }
    };

    VerificationResult {
        name: claim.name.clone(),
        span: claim.span,
        outcome,
    }
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
            parse_document(":claim pythag\n  sin(x)**2 + cos(x)**2 = 1").unwrap();
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
}
