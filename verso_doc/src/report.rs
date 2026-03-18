use crate::dim::DimOutcome;
use crate::verify::{Outcome, VerificationReport, VerificationResult};
use std::fmt;

pub struct ReportFormatter<'a> {
    pub report: &'a VerificationReport,
    pub filename: &'a str,
}

impl<'a> fmt::Display for ReportFormatter<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "\n--- {} ---\n", self.filename)?;

        for result in &self.report.results {
            write_result(f, result)?;
            if !result.units.is_empty() {
                writeln!(f, "    \x1b[36munits: {}\x1b[0m", result.units.join(", "))?;
            }
            if let Some(ref dim) = result.dim_outcome {
                write_dim_outcome(f, dim)?;
            }
        }

        let total = self.report.results.len();
        let passed = self.report.pass_count();
        let failed = self.report.fail_count();

        writeln!(f)?;
        if failed == 0 {
            writeln!(f, "\x1b[32m{} passed\x1b[0m ({} total)", passed, total)?;
        } else {
            writeln!(
                f,
                "\x1b[32m{} passed\x1b[0m, \x1b[31m{} failed\x1b[0m ({} total)",
                passed, failed, total
            )?;
        }

        Ok(())
    }
}

fn write_result(f: &mut fmt::Formatter<'_>, result: &VerificationResult) -> fmt::Result {
    match &result.outcome {
        Outcome::Pass => {
            writeln!(
                f,
                "  \x1b[32m\u{2713}\x1b[0m {} (line {})",
                result.name, result.span.line
            )
        }
        Outcome::NumericalPass { samples, .. } => {
            writeln!(
                f,
                "  \x1b[33m~\x1b[0m {} (numerical, {} samples, line {})",
                result.name, samples, result.span.line
            )
        }
        Outcome::Fail { residual } => {
            writeln!(
                f,
                "  \x1b[31m\u{2717}\x1b[0m {} (line {})",
                result.name, result.span.line
            )?;
            writeln!(f, "    residual: {}", residual)
        }
        Outcome::ProofPass { steps } => {
            writeln!(
                f,
                "  \x1b[32m\u{2713}\x1b[0m {} ({} steps, line {})",
                result.name, steps, result.span.line
            )
        }
        Outcome::ProofStepFail {
            step_index,
            from,
            to,
            residual,
            step_span,
        } => {
            writeln!(
                f,
                "  \x1b[31m\u{2717}\x1b[0m {} (step {}, line {})",
                result.name, step_index, step_span.line
            )?;
            writeln!(f, "    from: {}", from)?;
            writeln!(f, "      to: {}", to)?;
            writeln!(f, "    residual: {}", residual)
        }
        Outcome::ExpectFailPass => {
            writeln!(
                f,
                "  \x1b[32m\u{2713}\x1b[0m {} (expect_fail, line {})",
                result.name, result.span.line
            )
        }
        Outcome::ExpectFailFail => {
            writeln!(
                f,
                "  \x1b[31m\u{2717}\x1b[0m {} (expect_fail: all checks passed unexpectedly, line {})",
                result.name, result.span.line
            )
        }
    }
}

fn write_dim_outcome(f: &mut fmt::Formatter<'_>, dim: &DimOutcome) -> fmt::Result {
    match dim {
        DimOutcome::Pass => Ok(()), // no extra output needed
        DimOutcome::Skipped { undeclared } => {
            writeln!(
                f,
                "    \x1b[90mdim: skipped (undeclared: {})\x1b[0m",
                undeclared.join(", ")
            )
        }
        DimOutcome::LhsRhsMismatch { lhs, rhs } => {
            writeln!(
                f,
                "    \x1b[31mdim: mismatch — lhs {}, rhs {}\x1b[0m",
                lhs, rhs
            )
        }
        DimOutcome::ExprError { side, error } => {
            writeln!(f, "    \x1b[31mdim: error in {} — {}\x1b[0m", side, error)
        }
    }
}
