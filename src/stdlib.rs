use crate::error::{SaftError, SaftResult};
use crate::value::Value;

pub type BuiltinFn = fn(Vec<Value>) -> SaftResult<Value>;

#[derive(Clone, Copy)]
pub struct BuiltinSpec {
    pub name: &'static str,
    pub arity: usize,
    pub func: BuiltinFn,
}

pub const BUILTIN_NAMES: &[&str] = &["upper", "print", "len", "type"];

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
        other => {
            return Err(SaftError::new(format!(
                "len expects string/list/tuple/object, got {}",
                other.type_name()
            )))
        }
    };
    Ok(Value::Int(length))
}

fn builtin_type(args: Vec<Value>) -> SaftResult<Value> {
    let arg = take_one_arg(args, "type")?;
    Ok(Value::String(arg.type_name().to_string()))
}
