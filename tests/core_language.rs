use orangensaft::{check_source, run_source};

#[test]
fn runs_basic_assignments_example() {
    let source = include_str!("../examples/01_vanilla_assignments.saft");
    let result = run_source(source);
    assert!(
        result.is_ok(),
        "expected basic assignments example to run, got {result:?}"
    );
}

#[test]
fn resolver_reports_undefined_name() {
    let source = "assert missing_name == 1\n";
    let err = check_source(source).expect_err("expected undefined name error");
    assert!(err.message.contains("undefined name 'missing_name'"));
}

#[test]
fn runtime_enforces_assignment_schema() {
    let source = "x: int = \"not an int\"\n";
    let err = run_source(source).expect_err("expected schema validation failure");
    assert!(err.message.contains("schema validation failed"));
}

#[test]
fn multiline_object_schema_assignment_parses() {
    let source = r#"
report: {
    score: float,
    meta: {
        title: string,
        tags: [string]
    }
} = {
    score: 1.5,
    meta: {title: "ok", tags: ["a", "b"]}
}

assert report.meta.title == "ok"
"#;

    let result = run_source(source);
    assert!(
        result.is_ok(),
        "expected multiline object schema assignment to run, got {result:?}"
    );
}
