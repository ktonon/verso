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
