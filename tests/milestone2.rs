use orangensaft::provider::SequenceProvider;
use orangensaft::{run_source, run_source_with_provider};

#[test]
fn runs_example_02_with_prompt_assignment() {
    let source = include_str!("../examples/02_add_numbers.saft");
    let result = run_source(source);
    assert!(result.is_ok(), "expected example 02 to run, got {result:?}");
}

#[test]
fn runs_example_03_with_prompt_array_output() {
    let source = include_str!("../examples/03_another_array_op.saft");
    let result = run_source(source);
    assert!(result.is_ok(), "expected example 03 to run, got {result:?}");
}

#[test]
fn unannotated_prompt_result_stays_string() {
    let source = r#"
x = 2
y = 2
z = $
    hey what's {x} + {y}
$
assert z == "4"
"#;
    let result = run_source(source);
    assert!(
        result.is_ok(),
        "expected unannotated prompt assignment to return string, got {result:?}"
    );
}

#[test]
fn typed_prompt_assignment_requires_json() {
    let source = r#"
x: int = $
    return anything
$
"#;

    let err = run_source_with_provider(
        source,
        Box::new(SequenceProvider::from_texts(vec![
            "not-json".to_string(),
            "still-not-json".to_string(),
        ])),
    )
    .expect_err("expected JSON parse failure for typed prompt assignment");

    assert!(err.message.contains("after repair attempt"));
}

#[test]
fn typed_prompt_assignment_can_repair_once() {
    let source = r#"
x: int = $
    return the number 7
$
assert x == 7
"#;

    let result = run_source_with_provider(
        source,
        Box::new(SequenceProvider::from_texts(vec![
            "not-json".to_string(),
            "7".to_string(),
        ])),
    );

    assert!(
        result.is_ok(),
        "expected typed prompt repair to recover, got {result:?}"
    );
}
