use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use orangensaft::error::SaftResult;
use orangensaft::provider::{PromptProvider, PromptRequest, PromptResponse};
use orangensaft::run_source_with_provider;

#[test]
fn polars_dataframe_builtins_work() {
    let csv_path = temp_csv_path("polars_builtins");
    fs::write(
        &csv_path,
        "city,temp,score\nalpha,10,0.2\nbeta,20,0.8\ngamma,30,0.6\n",
    )
    .expect("failed to write csv test fixture");

    let source = format!(
        r#"
df = read("{path}")
assert type(df) == "dataframe"

s = shape(df)
assert s.0 == 3
assert s.1 == 3

cols = columns(df)
assert cols[0] == "city"
assert cols[1] == "temp"
assert cols[2] == "score"

assert mean(df, "temp") == 20.0
assert sum(df, "temp") == 60.0
assert min(df, "score") == 0.2
assert max(df, "score") == 0.8

preview = head(df)
assert len(preview) == 3
assert preview[0]["city"] == "alpha"

small = select(df, ["city", "score"])
assert shape(small).0 == 3
assert shape(small).1 == 2
"#,
        path = saft_string(csv_path.as_path()),
    );

    let result = run_source_with_provider(source.as_str(), Box::new(InspectingProvider::default()));
    let _ = fs::remove_file(&csv_path);

    assert!(
        result.is_ok(),
        "expected polars dataframe builtins to work, got {result:?}"
    );
}

#[test]
fn dataframe_prompt_interpolation_includes_context_block() {
    let csv_path = temp_csv_path("prompt_context");
    fs::write(&csv_path, "team,points,assists\na,10,5\nb,20,9\nc,15,6\n")
        .expect("failed to write csv test fixture");

    let source = format!(
        r#"
df = read("{path}")

answer = $
    which column from {{df}} has highest average
$

assert answer == "points"
"#,
        path = saft_string(csv_path.as_path()),
    );

    let result = run_source_with_provider(source.as_str(), Box::new(InspectingProvider::default()));
    let _ = fs::remove_file(&csv_path);

    assert!(
        result.is_ok(),
        "expected dataframe prompt context interpolation to work, got {result:?}"
    );
}

#[derive(Default)]
struct InspectingProvider;

impl PromptProvider for InspectingProvider {
    fn complete(&mut self, request: PromptRequest) -> SaftResult<PromptResponse> {
        if request.prompt.contains("which column from") {
            assert!(
                request.prompt.contains("\"__kind\":\"dataframe_context\""),
                "prompt should contain dataframe context marker, got:\n{}",
                request.prompt
            );
            assert!(
                request.prompt.contains("\"shape\""),
                "prompt should include dataframe shape, got:\n{}",
                request.prompt
            );
            assert!(
                request.prompt.contains("\"numeric_profile\""),
                "prompt should include numeric profile, got:\n{}",
                request.prompt
            );
            assert!(
                request.prompt.contains("\"points\""),
                "prompt should include concrete column names, got:\n{}",
                request.prompt
            );

            return Ok(PromptResponse::FinalText("points".to_string()));
        }

        Ok(PromptResponse::FinalText(String::new()))
    }
}

fn temp_csv_path(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "orangensaft_{prefix}_{}_{}.csv",
        std::process::id(),
        nanos
    ))
}

fn saft_string(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}
