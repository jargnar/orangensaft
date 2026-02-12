use orangensaft::{check_source, run_source};

#[test]
fn runs_example_01() {
    let source = include_str!("../examples/01_vanilla_assignments.saft");
    let result = run_source(source);
    assert!(result.is_ok(), "expected example 01 to run, got {result:?}");
}

#[test]
fn check_reports_undefined_names() {
    let source = "assert missing_name == 1\n";
    let err = check_source(source).expect_err("expected undefined name error");
    assert!(err.message.contains("undefined name 'missing_name'"));
}

#[test]
fn run_enforces_strict_schema_validation() {
    let source = "x: int = \"not an int\"\n";
    let err = run_source(source).expect_err("expected schema validation failure");
    assert!(err.message.contains("schema validation failed"));
}
