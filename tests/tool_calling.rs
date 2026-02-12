use orangensaft::run_source;

#[test]
fn runs_function_map_tool_calling_example() {
    let source = include_str!("../examples/06_function_map.saft");
    let result = run_source(source);
    assert!(
        result.is_ok(),
        "expected function map tool-calling example to run, got {result:?}"
    );
}

#[test]
fn runs_object_returning_tool_example() {
    let source = include_str!("../examples/07_function_return_objects.saft");
    let result = run_source(source);
    assert!(
        result.is_ok(),
        "expected object-returning tool example to run, got {result:?}"
    );
}

#[test]
fn runs_conditional_tool_choice_example() {
    let source = include_str!("../examples/08_choose_function.saft");
    let result = run_source(source);
    assert!(
        result.is_ok(),
        "expected conditional tool-choice example to run, got {result:?}"
    );
}

#[test]
fn runs_tool_alias_and_capability_example() {
    let source = include_str!("../examples/09_llm_tool_alias_and_capability.saft");
    let result = run_source(source);
    assert!(
        result.is_ok(),
        "expected tool alias/capability example to run, got {result:?}"
    );
}

#[test]
fn runs_single_pair_tool_call_example() {
    let source = include_str!("../examples/10_another_func_call.saft");
    let result = run_source(source);
    assert!(
        result.is_ok(),
        "expected single pair tool-call example to run, got {result:?}"
    );
}
