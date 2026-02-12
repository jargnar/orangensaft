use std::collections::BTreeMap;
use std::path::Path;

use polars::prelude::{AnyValue, ChunkAgg, CsvReader, DataType, SerReader};

use crate::error::{SaftError, SaftResult};
use crate::value::{DataFrameValue, Value};

pub type BuiltinFn = fn(Vec<Value>) -> SaftResult<Value>;

#[derive(Clone, Copy)]
pub struct BuiltinSpec {
    pub name: &'static str,
    pub arity: usize,
    pub func: BuiltinFn,
}

const DEFAULT_HEAD_ROWS: usize = 5;

pub const BUILTIN_NAMES: &[&str] = &[
    "upper", "print", "len", "type", "read", "shape", "columns", "head", "select", "mean", "sum",
    "min", "max",
];

pub const BUILTINS: &[BuiltinSpec] = &[
    BuiltinSpec {
        name: "upper",
        arity: 1,
        func: builtin_upper,
    },
    BuiltinSpec {
        name: "print",
        arity: 1,
        func: builtin_print,
    },
    BuiltinSpec {
        name: "len",
        arity: 1,
        func: builtin_len,
    },
    BuiltinSpec {
        name: "type",
        arity: 1,
        func: builtin_type,
    },
    BuiltinSpec {
        name: "read",
        arity: 1,
        func: builtin_read,
    },
    BuiltinSpec {
        name: "shape",
        arity: 1,
        func: builtin_shape,
    },
    BuiltinSpec {
        name: "columns",
        arity: 1,
        func: builtin_columns,
    },
    BuiltinSpec {
        name: "head",
        arity: 1,
        func: builtin_head,
    },
    BuiltinSpec {
        name: "select",
        arity: 2,
        func: builtin_select,
    },
    BuiltinSpec {
        name: "mean",
        arity: 2,
        func: builtin_mean,
    },
    BuiltinSpec {
        name: "sum",
        arity: 2,
        func: builtin_sum,
    },
    BuiltinSpec {
        name: "min",
        arity: 2,
        func: builtin_min,
    },
    BuiltinSpec {
        name: "max",
        arity: 2,
        func: builtin_max,
    },
];

fn take_one_arg(args: Vec<Value>, name: &str) -> SaftResult<Value> {
    if args.len() != 1 {
        return Err(SaftError::new(format!("{name} expects one argument")));
    }
    Ok(args
        .into_iter()
        .next()
        .expect("len check above guarantees one argument"))
}

fn take_two_args(args: Vec<Value>, name: &str) -> SaftResult<(Value, Value)> {
    if args.len() != 2 {
        return Err(SaftError::new(format!("{name} expects two arguments")));
    }
    let mut iter = args.into_iter();
    let first = iter
        .next()
        .expect("len check above guarantees two arguments");
    let second = iter
        .next()
        .expect("len check above guarantees two arguments");
    Ok((first, second))
}

fn expect_dataframe(value: Value, name: &str) -> SaftResult<DataFrameValue> {
    match value {
        Value::DataFrame(df) => Ok(df),
        other => Err(SaftError::new(format!(
            "{name} expects dataframe, got {}",
            other.type_name()
        ))),
    }
}

fn expect_string(value: Value, name: &str) -> SaftResult<String> {
    match value {
        Value::String(text) => Ok(text),
        other => Err(SaftError::new(format!(
            "{name} expects string, got {}",
            other.type_name()
        ))),
    }
}

fn expect_string_list(value: Value, name: &str) -> SaftResult<Vec<String>> {
    match value {
        Value::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                match item {
                    Value::String(text) => out.push(text),
                    other => {
                        return Err(SaftError::new(format!(
                            "{name} expects list[string], got list containing {}",
                            other.type_name()
                        )));
                    }
                }
            }
            Ok(out)
        }
        other => Err(SaftError::new(format!(
            "{name} expects list[string], got {}",
            other.type_name()
        ))),
    }
}

fn builtin_upper(args: Vec<Value>) -> SaftResult<Value> {
    let arg = take_one_arg(args, "upper")?;
    match arg {
        Value::String(value) => Ok(Value::String(value.to_uppercase())),
        other => Err(SaftError::new(format!(
            "upper expects string, got {}",
            other.type_name()
        ))),
    }
}

fn builtin_print(args: Vec<Value>) -> SaftResult<Value> {
    let arg = take_one_arg(args, "print")?;
    match arg {
        Value::String(text) => println!("{text}"),
        other => println!("{other}"),
    }
    Ok(Value::Nil)
}

fn builtin_len(args: Vec<Value>) -> SaftResult<Value> {
    let arg = take_one_arg(args, "len")?;
    let length = match arg {
        Value::String(text) => text.chars().count() as i64,
        Value::List(items) => items.len() as i64,
        Value::Tuple(items) => items.len() as i64,
        Value::Object(map) => map.len() as i64,
        Value::DataFrame(df) => df.rows() as i64,
        other => {
            return Err(SaftError::new(format!(
                "len expects string/list/tuple/object/dataframe, got {}",
                other.type_name()
            )));
        }
    };
    Ok(Value::Int(length))
}

fn builtin_type(args: Vec<Value>) -> SaftResult<Value> {
    let arg = take_one_arg(args, "type")?;
    Ok(Value::String(arg.type_name().to_string()))
}

fn builtin_read(args: Vec<Value>) -> SaftResult<Value> {
    let path = expect_string(take_one_arg(args, "read")?, "read")?;
    let normalized_path = Path::new(&path);
    let frame = CsvReader::from_path(normalized_path)
        .map_err(|err| SaftError::new(format!("read could not open csv '{path}': {err}")))?
        .has_header(true)
        .finish()
        .map_err(|err| SaftError::new(format!("read failed to parse csv '{path}': {err}")))?;

    Ok(Value::DataFrame(DataFrameValue::new(frame)))
}

fn builtin_shape(args: Vec<Value>) -> SaftResult<Value> {
    let df = expect_dataframe(take_one_arg(args, "shape")?, "shape")?;
    Ok(Value::Tuple(vec![
        Value::Int(df.rows() as i64),
        Value::Int(df.cols() as i64),
    ]))
}

fn builtin_columns(args: Vec<Value>) -> SaftResult<Value> {
    let df = expect_dataframe(take_one_arg(args, "columns")?, "columns")?;
    let names = df
        .frame()
        .get_column_names()
        .into_iter()
        .map(|name| Value::String(name.to_string()))
        .collect::<Vec<_>>();
    Ok(Value::List(names))
}

fn builtin_head(args: Vec<Value>) -> SaftResult<Value> {
    let df = expect_dataframe(take_one_arg(args, "head")?, "head")?;
    let preview = dataframe_rows(df.frame(), DEFAULT_HEAD_ROWS)?;
    Ok(Value::List(preview))
}

fn builtin_select(args: Vec<Value>) -> SaftResult<Value> {
    let (df_value, cols_value) = take_two_args(args, "select")?;
    let df = expect_dataframe(df_value, "select")?;
    let cols = expect_string_list(cols_value, "select")?;
    if cols.is_empty() {
        return Err(SaftError::new("select expects at least one column name"));
    }

    let col_refs = cols.iter().map(String::as_str).collect::<Vec<_>>();
    let selected = df
        .frame()
        .select(col_refs)
        .map_err(|err| SaftError::new(format!("select failed: {err}")))?;
    Ok(Value::DataFrame(DataFrameValue::new(selected)))
}

fn builtin_mean(args: Vec<Value>) -> SaftResult<Value> {
    let (df_value, column_value) = take_two_args(args, "mean")?;
    let df = expect_dataframe(df_value, "mean")?;
    let column = expect_string(column_value, "mean")?;
    let result = numeric_aggregate(df.frame(), &column, "mean", |col| col.mean())?;
    Ok(Value::Float(result))
}

fn builtin_sum(args: Vec<Value>) -> SaftResult<Value> {
    let (df_value, column_value) = take_two_args(args, "sum")?;
    let df = expect_dataframe(df_value, "sum")?;
    let column = expect_string(column_value, "sum")?;
    let result = numeric_aggregate(df.frame(), &column, "sum", |col| col.sum())?;
    Ok(Value::Float(result))
}

fn builtin_min(args: Vec<Value>) -> SaftResult<Value> {
    let (df_value, column_value) = take_two_args(args, "min")?;
    let df = expect_dataframe(df_value, "min")?;
    let column = expect_string(column_value, "min")?;
    let result = numeric_aggregate(df.frame(), &column, "min", |col| col.min())?;
    Ok(Value::Float(result))
}

fn builtin_max(args: Vec<Value>) -> SaftResult<Value> {
    let (df_value, column_value) = take_two_args(args, "max")?;
    let df = expect_dataframe(df_value, "max")?;
    let column = expect_string(column_value, "max")?;
    let result = numeric_aggregate(df.frame(), &column, "max", |col| col.max())?;
    Ok(Value::Float(result))
}

fn numeric_aggregate(
    frame: &polars::prelude::DataFrame,
    column: &str,
    op_name: &str,
    op: impl FnOnce(&polars::prelude::Float64Chunked) -> Option<f64>,
) -> SaftResult<f64> {
    let series = frame
        .column(column)
        .map_err(|err| SaftError::new(format!("{op_name} failed: {err}")))?;

    let casted = series
        .cast(&DataType::Float64)
        .map_err(|_| SaftError::new(format!("{op_name} expects numeric column '{column}'")))?;
    let as_float = casted
        .f64()
        .map_err(|_| SaftError::new(format!("{op_name} expects numeric column '{column}'")))?;

    op(as_float).ok_or_else(|| {
        SaftError::new(format!(
            "{op_name} failed: column '{column}' has no non-null numeric values"
        ))
    })
}

fn dataframe_rows(frame: &polars::prelude::DataFrame, max_rows: usize) -> SaftResult<Vec<Value>> {
    let rows = std::cmp::min(frame.height(), max_rows);
    let mut out = Vec::with_capacity(rows);

    for row_idx in 0..rows {
        let mut row = BTreeMap::new();
        for column in frame.get_columns() {
            let name = column.name().to_string();
            let cell = anyvalue_to_value(
                column
                    .get(row_idx)
                    .map_err(|err| SaftError::new(format!("head failed to read cell: {err}")))?,
            );
            row.insert(name, cell);
        }
        out.push(Value::Object(row));
    }

    Ok(out)
}

fn anyvalue_to_value(value: AnyValue<'_>) -> Value {
    match value {
        AnyValue::Null => Value::Nil,
        AnyValue::Boolean(v) => Value::Bool(v),
        AnyValue::Int8(v) => Value::Int(v as i64),
        AnyValue::Int16(v) => Value::Int(v as i64),
        AnyValue::Int32(v) => Value::Int(v as i64),
        AnyValue::Int64(v) => Value::Int(v),
        AnyValue::UInt8(v) => Value::Int(v as i64),
        AnyValue::UInt16(v) => Value::Int(v as i64),
        AnyValue::UInt32(v) => Value::Int(v as i64),
        AnyValue::UInt64(v) => {
            if v <= i64::MAX as u64 {
                Value::Int(v as i64)
            } else {
                Value::Float(v as f64)
            }
        }
        AnyValue::Float32(v) => Value::Float(v as f64),
        AnyValue::Float64(v) => Value::Float(v),
        AnyValue::String(v) => Value::String(v.to_string()),
        AnyValue::StringOwned(v) => Value::String(v.to_string()),
        other => Value::String(other.to_string()),
    }
}
