use erd_doc::compile_tex::compile_to_tex;
use erd_doc::dim::DimOutcome;
use erd_doc::parse::parse_document;
use erd_doc::verify::{verify_document, Outcome};

fn load_fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("failed to read {}: {}", path, e))
}

#[test]
fn basic_algebra_all_pass() {
    let src = load_fixture("basic_algebra.erd");
    let doc = parse_document(&src).unwrap();
    let report = verify_document(&doc);
    for r in &report.results {
        assert!(
            r.passed(),
            "claim '{}' should pass but failed: {:?}",
            r.name,
            r.outcome
        );
    }
    assert_eq!(report.pass_count(), 5);
}

#[test]
fn trig_identities_all_pass() {
    let src = load_fixture("trig_identities.erd");
    let doc = parse_document(&src).unwrap();
    let report = verify_document(&doc);
    for r in &report.results {
        assert!(
            r.passed(),
            "claim '{}' should pass but failed: {:?}",
            r.name,
            r.outcome
        );
    }
    assert_eq!(report.pass_count(), 4);
}

#[test]
fn should_fail_detects_wrong_claim() {
    let src = load_fixture("should_fail.erd");
    let doc = parse_document(&src).unwrap();
    let report = verify_document(&doc);
    assert_eq!(report.fail_count(), 1);
    assert_eq!(report.results[0].name, "wrong_identity");
}

#[test]
fn proof_chain_passes() {
    let src = load_fixture("proof_chain.erd");
    let doc = parse_document(&src).unwrap();
    let report = verify_document(&doc);
    for r in &report.results {
        assert!(
            r.passed(),
            "'{}' should pass but failed: {:?}",
            r.name,
            r.outcome
        );
    }
    // 1 claim + 1 proof
    assert_eq!(report.pass_count(), 2);
}

#[test]
fn full_document_passes() {
    let src = load_fixture("full_document.erd");
    let doc = parse_document(&src).unwrap();
    let report = verify_document(&doc);
    for r in &report.results {
        assert!(
            r.passed(),
            "'{}' should pass but failed: {:?}",
            r.name,
            r.outcome
        );
    }
    // 2 claims + 1 proof
    assert_eq!(report.pass_count(), 3);
}

#[test]
fn full_document_compiles_to_tex() {
    let src = load_fixture("full_document.erd");
    let doc = parse_document(&src).unwrap();
    let tex = compile_to_tex(&doc);

    assert!(tex.contains("\\documentclass[11pt]{article}"));
    assert!(tex.contains("\\section{Trigonometric Identities}"));
    assert!(tex.contains("$\\sin{x}^{2} + \\cos{x}^{2}$"));
    assert!(tex.contains("\\eqref{eq:pythagorean}"));
    assert!(tex.contains("\\label{eq:pythagorean}"));
    assert!(tex.contains("\\label{eq:double_angle_cos}"));
    assert!(tex.contains("\\begin{align*}"));
    assert!(tex.contains("\\end{document}"));
}

#[test]
fn numerical_fallback_passes() {
    let src = load_fixture("numerical_fallback.erd");
    let doc = parse_document(&src).unwrap();
    let report = verify_document(&doc);
    for r in &report.results {
        assert!(
            r.passed(),
            "'{}' should pass (at least numerically) but failed: {:?}",
            r.name,
            r.outcome
        );
    }
    assert_eq!(report.pass_count(), 2);
    // These should be NumericalPass since symbolic engine can't prove them
    for r in &report.results {
        assert!(
            matches!(r.outcome, Outcome::NumericalPass { .. }),
            "'{}' should be NumericalPass, got {:?}",
            r.name,
            r.outcome
        );
    }
}

#[test]
fn dimensional_analysis_passes() {
    let src = load_fixture("dimensional.erd");
    let doc = parse_document(&src).unwrap();
    let report = verify_document(&doc);
    assert_eq!(report.pass_count(), 3);
    for r in &report.results {
        assert!(
            r.passed(),
            "'{}' should pass but failed: {:?} dim: {:?}",
            r.name,
            r.outcome,
            r.dim_outcome
        );
        // All claims should have dimension checking enabled and passing
        assert!(
            matches!(r.dim_outcome, Some(DimOutcome::Pass)),
            "'{}' should have DimOutcome::Pass, got {:?}",
            r.name,
            r.dim_outcome
        );
    }
}

#[test]
fn dimensional_mismatch_detected() {
    let src = load_fixture("dim_fail.erd");
    let doc = parse_document(&src).unwrap();
    let report = verify_document(&doc);
    // The claim x = t fails symbolically AND dimensionally
    assert_eq!(report.fail_count(), 1);
    let r = &report.results[0];
    assert_eq!(r.name, "wrong_units");
    assert!(
        matches!(r.dim_outcome, Some(DimOutcome::LhsRhsMismatch { .. })),
        "expected dimension mismatch, got {:?}",
        r.dim_outcome
    );
}

#[test]
fn unit_annotations_pass_dim_check() {
    let src = load_fixture("units.erd");
    let doc = parse_document(&src).unwrap();
    let report = verify_document(&doc);
    for r in &report.results {
        assert!(
            r.passed(),
            "'{}' should pass but failed: {:?} dim: {:?}",
            r.name,
            r.outcome,
            r.dim_outcome
        );
        // Claims with :dim declarations and unit quantities should have dimension checking
        assert!(
            matches!(r.dim_outcome, Some(DimOutcome::Pass)),
            "'{}' should have DimOutcome::Pass, got {:?}",
            r.name,
            r.dim_outcome
        );
    }
    assert_eq!(report.pass_count(), 2);
}

#[test]
fn no_dim_declarations_skips_dim_check() {
    // Documents without :dim blocks should have dim_outcome = None
    let src = load_fixture("basic_algebra.erd");
    let doc = parse_document(&src).unwrap();
    let report = verify_document(&doc);
    for r in &report.results {
        assert!(
            r.dim_outcome.is_none(),
            "'{}' should have no dim_outcome without :dim declarations",
            r.name
        );
    }
}
