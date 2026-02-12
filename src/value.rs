use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use polars::prelude::DataFrame;

pub type FunctionId = usize;

#[derive(Debug, Clone)]
pub struct DataFrameValue {
    frame: Arc<DataFrame>,
}

impl DataFrameValue {
    pub fn new(frame: DataFrame) -> Self {
        Self {
            frame: Arc::new(frame),
        }
    }

    pub fn frame(&self) -> &DataFrame {
        self.frame.as_ref()
    }

    pub fn rows(&self) -> usize {
        self.frame.height()
    }

    pub fn cols(&self) -> usize {
        self.frame.width()
    }
}

impl PartialEq for DataFrameValue {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.frame, &other.frame)
    }
}

#[derive(Debug, Clone)]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    List(Vec<Value>),
    Tuple(Vec<Value>),
    Object(BTreeMap<String, Value>),
    DataFrame(DataFrameValue),
    Function(FunctionId),
    Nil,
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::Bool(_) => "bool",
            Value::String(_) => "string",
            Value::List(_) => "list",
            Value::Tuple(_) => "tuple",
            Value::Object(_) => "object",
            Value::DataFrame(_) => "dataframe",
            Value::Function(_) => "function",
            Value::Nil => "nil",
        }
    }

    pub fn is_truthy(&self) -> bool {
        !matches!(self, Value::Bool(false) | Value::Nil)
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::String(a), Value::String(b)) => a == b,
            (Value::List(a), Value::List(b)) => a == b,
            (Value::Tuple(a), Value::Tuple(b)) => a == b,
            (Value::Object(a), Value::Object(b)) => a == b,
            (Value::DataFrame(a), Value::DataFrame(b)) => a == b,
            (Value::Function(a), Value::Function(b)) => a == b,
            (Value::Nil, Value::Nil) => true,
            _ => false,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{v}"),
            Value::Float(v) => write!(f, "{v}"),
            Value::Bool(v) => write!(f, "{v}"),
            Value::String(v) => write!(f, "\"{}\"", v),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            Value::Tuple(items) => {
                write!(f, "(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, ")")
            }
            Value::Object(map) => {
                write!(f, "{{")?;
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
            Value::DataFrame(df) => write!(f, "<dataframe rows={} cols={}>", df.rows(), df.cols()),
            Value::Function(id) => write!(f, "<function:{id}>"),
            Value::Nil => write!(f, "nil"),
        }
    }
}
