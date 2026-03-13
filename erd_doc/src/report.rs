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
    }
}
