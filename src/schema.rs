use crate::ast::SchemaExpr;
use crate::value::Value;
use serde_json::{Map as JsonMap, Value as JsonValue};

pub fn validate(value: &Value, schema: &SchemaExpr) -> Result<(), String> {
    validate_inner(value, schema, "value")
}

fn validate_inner(value: &Value, schema: &SchemaExpr, path: &str) -> Result<(), String> {
    match schema {
        SchemaExpr::Any => Ok(()),
        SchemaExpr::Int => match value {
            Value::Int(_) => Ok(()),
            _ => Err(type_mismatch(path, schema, value)),
        },
        SchemaExpr::Float => match value {
            Value::Float(_) => Ok(()),
            _ => Err(type_mismatch(path, schema, value)),
        },
        SchemaExpr::Bool => match value {
            Value::Bool(_) => Ok(()),
            _ => Err(type_mismatch(path, schema, value)),
        },
        SchemaExpr::String => match value {
            Value::String(_) => Ok(()),
            _ => Err(type_mismatch(path, schema, value)),
        },
        SchemaExpr::List(item_schema) => match value {
            Value::List(items) => {
                for (idx, item) in items.iter().enumerate() {
                    validate_inner(item, item_schema, &format!("{path}[{idx}]"))?;
                }
                Ok(())
            }
            _ => Err(type_mismatch(path, schema, value)),
        },
        SchemaExpr::Tuple(item_schemas) => match value {
            Value::Tuple(items) => {
                if items.len() != item_schemas.len() {
                    return Err(format!(
                        "{path}: expected tuple length {}, got {}",
                        item_schemas.len(),
                        items.len()
                    ));
                }

                for (idx, (item, item_schema)) in items.iter().zip(item_schemas.iter()).enumerate()
                {
                    validate_inner(item, item_schema, &format!("{path}.{idx}"))?;
                }
                Ok(())
            }
            _ => Err(type_mismatch(path, schema, value)),
        },
        SchemaExpr::Object(fields) => match value {
            Value::Object(map) => {
                for field in fields {
                    let Some(field_value) = map.get(&field.name) else {
                        return Err(format!("{path}: missing field '{}'", field.name));
                    };
                    validate_inner(
                        field_value,
                        &field.schema,
                        &format!("{path}.{}", field.name),
                    )?;
                }

                for key in map.keys() {
                    if !fields.iter().any(|field| field.name == *key) {
                        return Err(format!("{path}: unexpected field '{key}'"));
                    }
                }

                Ok(())
            }
            _ => Err(type_mismatch(path, schema, value)),
        },
        SchemaExpr::Union(variants) => {
            let mut variant_errors = Vec::new();
            for variant in variants {
                match validate_inner(value, variant, path) {
                    Ok(()) => return Ok(()),
                    Err(err) => variant_errors.push(err),
                }
            }

            Err(format!(
                "{path}: value did not match any union variant ({})",
                variant_errors.join("; ")
            ))
        }
        SchemaExpr::Optional(inner) => {
            if matches!(value, Value::Nil) {
                Ok(())
            } else {
                validate_inner(value, inner, path)
            }
        }
    }
}

fn type_mismatch(path: &str, schema: &SchemaExpr, value: &Value) -> String {
    format!(
        "{path}: expected {}, got {}",
        schema_to_string(schema),
        value.type_name()
    )
}

pub fn schema_to_string(schema: &SchemaExpr) -> String {
    match schema {
        SchemaExpr::Any => "any".to_string(),
        SchemaExpr::Int => "int".to_string(),
        SchemaExpr::Float => "float".to_string(),
        SchemaExpr::Bool => "bool".to_string(),
        SchemaExpr::String => "string".to_string(),
        SchemaExpr::List(item) => format!("[{}]", schema_to_string(item)),
        SchemaExpr::Tuple(items) => {
            let body = items
                .iter()
                .map(schema_to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!("({body})")
        }
        SchemaExpr::Object(fields) => {
            let body = fields
                .iter()
                .map(|field| format!("{}: {}", field.name, schema_to_string(&field.schema)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{{body}}}")
        }
        SchemaExpr::Union(variants) => variants
            .iter()
            .map(schema_to_string)
            .collect::<Vec<_>>()
            .join(" | "),
        SchemaExpr::Optional(inner) => format!("{}?", schema_to_string(inner)),
    }
}

pub fn to_json_schema(schema: &SchemaExpr) -> JsonValue {
    match schema {
        SchemaExpr::Any => JsonValue::Object(JsonMap::new()),
        SchemaExpr::Int => json_type("integer"),
        SchemaExpr::Float => json_type("number"),
        SchemaExpr::Bool => json_type("boolean"),
        SchemaExpr::String => json_type("string"),
        SchemaExpr::List(inner) => {
            let mut obj = JsonMap::new();
            obj.insert("type".to_string(), JsonValue::String("array".to_string()));
            obj.insert("items".to_string(), to_json_schema(inner));
            JsonValue::Object(obj)
        }
        SchemaExpr::Tuple(items) => {
            let mut obj = JsonMap::new();
            obj.insert("type".to_string(), JsonValue::String("array".to_string()));
            obj.insert(
                "prefixItems".to_string(),
                JsonValue::Array(items.iter().map(to_json_schema).collect()),
            );
            obj.insert(
                "minItems".to_string(),
                JsonValue::Number((items.len() as u64).into()),
            );
            obj.insert(
                "maxItems".to_string(),
                JsonValue::Number((items.len() as u64).into()),
            );
            obj.insert("items".to_string(), JsonValue::Bool(false));
            JsonValue::Object(obj)
        }
        SchemaExpr::Object(fields) => {
            let mut properties = JsonMap::new();
            let mut required = Vec::with_capacity(fields.len());
            for field in fields {
                properties.insert(field.name.clone(), to_json_schema(&field.schema));
                required.push(JsonValue::String(field.name.clone()));
            }

            let mut obj = JsonMap::new();
            obj.insert("type".to_string(), JsonValue::String("object".to_string()));
            obj.insert("properties".to_string(), JsonValue::Object(properties));
            obj.insert("required".to_string(), JsonValue::Array(required));
            obj.insert("additionalProperties".to_string(), JsonValue::Bool(false));
            JsonValue::Object(obj)
        }
        SchemaExpr::Union(variants) => {
            let mut obj = JsonMap::new();
            obj.insert(
                "anyOf".to_string(),
                JsonValue::Array(variants.iter().map(to_json_schema).collect()),
            );
            JsonValue::Object(obj)
        }
        SchemaExpr::Optional(inner) => {
            let mut obj = JsonMap::new();
            obj.insert(
                "anyOf".to_string(),
                JsonValue::Array(vec![to_json_schema(inner), json_type("null")]),
            );
            JsonValue::Object(obj)
        }
    }
}

fn json_type(type_name: &str) -> JsonValue {
    let mut obj = JsonMap::new();
    obj.insert("type".to_string(), JsonValue::String(type_name.to_string()));
    JsonValue::Object(obj)
}
