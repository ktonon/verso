use erd_doc::compile_tex::compile_to_tex;
use erd_doc::parse::parse_document;
use erd_doc::verify::verify_document;

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

    assert!(tex.contains("\\documentclass{article}"));
    assert!(tex.contains("\\section{Trigonometric Identities}"));
    assert!(tex.contains("$\\sin{x}^{2} + \\cos{x}^{2}$"));
    assert!(tex.contains("\\eqref{eq:pythagorean}"));
    assert!(tex.contains("\\label{eq:pythagorean}"));
    assert!(tex.contains("\\label{eq:double_angle_cos}"));
    assert!(tex.contains("\\begin{align*}"));
    assert!(tex.contains("\\end{document}"));
}
