use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use orangensaft::run_source;

#[test]
fn runs_stdlib_basics_example() {
    let source = include_str!("../examples/12_stdlib_basics.saft");
    let result = run_source(source);
    assert!(
        result.is_ok(),
        "expected stdlib basics example to run, got {result:?}"
    );
}

#[test]
fn upper_builtin_transforms_text() {
    let source = r#"
out = upper("ship")
assert out == "SHIP"
"#;

    let result = run_source(source);
    assert!(result.is_ok(), "expected upper() to work, got {result:?}");
}

#[test]
fn len_builtin_supports_core_collection_types() {
    let source = r#"
assert len("hello") == 5
assert len([1, 2, 3]) == 3
assert len((1, 2, 3, 4)) == 4
assert len({a: 1, b: 2}) == 2
"#;

    let result = run_source(source);
    assert!(result.is_ok(), "expected len() to work, got {result:?}");
}

#[test]
fn type_builtin_reports_runtime_value_kind() {
    let source = r#"
assert type(1) == "int"
assert type(1.5) == "float"
assert type("x") == "string"
assert type([1, 2]) == "list"
assert type((1, "x")) == "tuple"
assert type({x: 1}) == "object"
assert type(nil) == "nil"
"#;

    let result = run_source(source);
    assert!(result.is_ok(), "expected type() to work, got {result:?}");
}

#[test]
fn print_builtin_returns_nil() {
    let source = r#"
result = print("hello from print")
assert result == nil
"#;

    let result = run_source(source);
    assert!(result.is_ok(), "expected print() to return nil, got {result:?}");
}

#[test]
fn print_builtin_writes_to_stdout_via_cli() {
    let binary = env!("CARGO_BIN_EXE_orangensaft");
    let script_path = temp_script_path("print_stdout");
    fs::write(&script_path, "print(\"hello stdout\")\n").expect("failed to write temp script");

    let output = Command::new(binary)
        .args(["run", script_path.to_string_lossy().as_ref(), "--provider", "none"])
        .output()
        .expect("failed to run orangensaft binary");

    let _ = fs::remove_file(&script_path);

    assert!(
        output.status.success(),
        "expected CLI run to succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("hello stdout"),
        "expected stdout to contain printed text, got: {stdout}"
    );
}

fn temp_script_path(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("orangensaft_{prefix}_{}_{}.saft", std::process::id(), nanos))
}
