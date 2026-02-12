use orangensaft::run_source;

#[test]
fn runs_example_06_function_map() {
    let source = include_str!("../examples/06_function_map.saft");
    let result = run_source(source);
    assert!(result.is_ok(), "expected example 06 to run, got {result:?}");
}

#[test]
fn runs_example_07_function_return_objects() {
    let source = include_str!("../examples/07_function_return_objects.saft");
    let result = run_source(source);
    assert!(result.is_ok(), "expected example 07 to run, got {result:?}");
}

#[test]
fn runs_example_08_choose_function() {
    let source = include_str!("../examples/08_choose_function.saft");
    let result = run_source(source);
    assert!(result.is_ok(), "expected example 08 to run, got {result:?}");
}

#[test]
fn runs_example_09_alias_and_capability() {
    let source = include_str!("../examples/09_llm_tool_alias_and_capability.saft");
    let result = run_source(source);
    assert!(result.is_ok(), "expected example 09 to run, got {result:?}");
}

#[test]
fn runs_example_10_another_func_call() {
    let source = include_str!("../examples/10_another_func_call.saft");
    let result = run_source(source);
    assert!(result.is_ok(), "expected example 10 to run, got {result:?}");
}
