use crate::ast::{
    BinaryOp, Expr, FnDef, FnParam, Pattern, Program, PromptExpr, PromptPart, SchemaExpr, Stmt,
    UnaryOp,
};
use crate::error::SaftResult;

const INDENT: &str = "    ";
const PREC_OR: u8 = 1;
const PREC_AND: u8 = 2;
const PREC_COMPARE: u8 = 3;
const PREC_ADD: u8 = 4;
const PREC_MUL: u8 = 5;
const PREC_UNARY: u8 = 6;
const PREC_POSTFIX: u8 = 7;

pub fn format_source(source: &str) -> SaftResult<String> {
    let tokens = crate::lexer::lex(source)?;
    let program = crate::parser::parse(tokens)?;
    Ok(format_program(&program))
}

pub fn format_program(program: &Program) -> String {
    let mut out = String::new();
    for stmt in &program.stmts {
        write_stmt(&mut out, stmt, 0);
    }
    out
}

fn write_stmt(out: &mut String, stmt: &Stmt, indent: usize) {
    match stmt {
        Stmt::FnDef(def) => write_fn_def(out, def, indent),
        Stmt::Assign {
            name,
            annotation,
            value,
            ..
        } => {
            write_indent(out, indent);
            out.push_str(name);
            if let Some(schema) = annotation {
                out.push_str(": ");
                out.push_str(&format_schema(schema));
            }
            out.push_str(" = ");
            out.push_str(&format_expr(value, 0));
            out.push('\n');
        }
        Stmt::If {
            cond,
            then_block,
            else_block,
            ..
        } => {
            write_indent(out, indent);
            out.push_str("if ");
            out.push_str(&format_expr(cond, 0));
            out.push_str(":\n");
            write_block(out, then_block, indent + 1);
            if let Some(block) = else_block {
                write_indent(out, indent);
                out.push_str("else:\n");
                write_block(out, block, indent + 1);
            }
        }
        Stmt::For {
            pattern,
            iter,
            body,
            ..
        } => {
            write_indent(out, indent);
            out.push_str("for ");
            out.push_str(&format_pattern(pattern));
            out.push_str(" in ");
            out.push_str(&format_expr(iter, 0));
            out.push_str(":\n");
            write_block(out, body, indent + 1);
        }
        Stmt::Return { value, .. } => {
            write_indent(out, indent);
            out.push_str("ret");
            if let Some(expr) = value {
                out.push(' ');
                out.push_str(&format_expr(expr, 0));
            }
            out.push('\n');
        }
        Stmt::Assert { expr, .. } => {
            write_indent(out, indent);
            out.push_str("assert ");
            out.push_str(&format_expr(expr, 0));
            out.push('\n');
        }
        Stmt::Expr { expr, .. } => {
            write_indent(out, indent);
            out.push_str(&format_expr(expr, 0));
            out.push('\n');
        }
    }
}

fn write_fn_def(out: &mut String, def: &FnDef, indent: usize) {
    write_indent(out, indent);
    out.push_str("f ");
    out.push_str(&def.name);
    out.push('(');
    out.push_str(
        &def.params
            .iter()
            .map(format_param)
            .collect::<Vec<_>>()
            .join(", "),
    );
    out.push(')');
    if let Some(schema) = &def.return_schema {
        out.push_str(" -> ");
        out.push_str(&format_schema(schema));
    }
    out.push_str(":\n");
    write_block(out, &def.body, indent + 1);
}

fn write_block(out: &mut String, block: &[Stmt], indent: usize) {
    for stmt in block {
        write_stmt(out, stmt, indent);
    }
}

fn format_param(param: &FnParam) -> String {
    if let Some(schema) = &param.schema {
        format!("{}: {}", param.name, format_schema(schema))
    } else {
        param.name.clone()
    }
}

fn format_pattern(pattern: &Pattern) -> String {
    match pattern {
        Pattern::Name(name) => name.clone(),
        Pattern::Tuple(names) => names.join(", "),
    }
}

fn format_schema(schema: &SchemaExpr) -> String {
    match schema {
        SchemaExpr::Any => "any".to_string(),
        SchemaExpr::Int => "int".to_string(),
        SchemaExpr::Float => "float".to_string(),
        SchemaExpr::Bool => "bool".to_string(),
        SchemaExpr::String => "string".to_string(),
        SchemaExpr::List(inner) => format!("[{}]", format_schema(inner)),
        SchemaExpr::Tuple(items) => format!(
            "({})",
            items
                .iter()
                .map(format_schema)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        SchemaExpr::Object(fields) => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|field| format!("{}: {}", field.name, format_schema(&field.schema)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        SchemaExpr::Union(variants) => variants
            .iter()
            .map(format_schema)
            .collect::<Vec<_>>()
            .join(" | "),
        SchemaExpr::Optional(inner) => {
            let inner_text = match inner.as_ref() {
                SchemaExpr::Union(_) => format!("({})", format_schema(inner)),
                _ => format_schema(inner),
            };
            format!("{inner_text}?")
        }
    }
}

fn format_expr(expr: &Expr, parent_prec: u8) -> String {
    match expr {
        Expr::Int(v, _) => v.to_string(),
        Expr::Float(v, _) => format_float(*v),
        Expr::Bool(v, _) => {
            if *v {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        Expr::Str(v, _) => serde_json::to_string(v).unwrap_or_else(|_| format!("\"{v}\"")),
        Expr::Nil(_) => "nil".to_string(),
        Expr::Var(name, _) => name.clone(),
        Expr::List(items, _) => format!(
            "[{}]",
            items
                .iter()
                .map(|item| format_expr(item, 0))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::Tuple(items, _) => format!(
            "({})",
            items
                .iter()
                .map(|item| format_expr(item, 0))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::Object(fields, _) => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(name, value)| format!("{name}: {}", format_expr(value, 0)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::Unary { op, expr, .. } => {
            let inner = format_expr(expr, PREC_UNARY);
            let body = match op {
                UnaryOp::Neg => format!("-{inner}"),
                UnaryOp::Not => format!("not {inner}"),
            };
            maybe_parenthesize(body, PREC_UNARY, parent_prec)
        }
        Expr::Binary {
            left, op, right, ..
        } => {
            let (prec, op_text) = binary_style(op);
            let left_text = format_expr(left, prec);
            let right_text = format_expr(right, prec + 1);
            let body = format!("{left_text} {op_text} {right_text}");
            maybe_parenthesize(body, prec, parent_prec)
        }
        Expr::Call { callee, args, .. } => {
            let callee_text = format_expr(callee, PREC_POSTFIX);
            let args_text = args
                .iter()
                .map(|arg| format_expr(arg, 0))
                .collect::<Vec<_>>()
                .join(", ");
            let body = format!("{callee_text}({args_text})");
            maybe_parenthesize(body, PREC_POSTFIX, parent_prec)
        }
        Expr::Index { target, index, .. } => {
            let target_text = format_expr(target, PREC_POSTFIX);
            let index_text = format_expr(index, 0);
            let body = format!("{target_text}[{index_text}]");
            maybe_parenthesize(body, PREC_POSTFIX, parent_prec)
        }
        Expr::Member { target, name, .. } => {
            let target_text = format_expr(target, PREC_POSTFIX);
            let body = format!("{target_text}.{name}");
            maybe_parenthesize(body, PREC_POSTFIX, parent_prec)
        }
        Expr::TupleIndex { target, index, .. } => {
            let target_text = format_expr(target, PREC_POSTFIX);
            let body = format!("{target_text}.{index}");
            maybe_parenthesize(body, PREC_POSTFIX, parent_prec)
        }
        Expr::Prompt(prompt) => format_prompt(prompt),
    }
}

fn format_prompt(prompt: &PromptExpr) -> String {
    let mut body = String::new();
    for part in &prompt.parts {
        match part {
            PromptPart::Text(text) => body.push_str(text),
            PromptPart::Interpolation(expr) => {
                body.push('{');
                body.push_str(&format_expr(expr, 0));
                body.push('}');
            }
        }
    }
    format!("${body}$")
}

fn maybe_parenthesize(text: String, my_prec: u8, parent_prec: u8) -> String {
    if my_prec < parent_prec {
        format!("({text})")
    } else {
        text
    }
}

fn binary_style(op: &BinaryOp) -> (u8, &'static str) {
    match op {
        BinaryOp::Or => (PREC_OR, "or"),
        BinaryOp::And => (PREC_AND, "and"),
        BinaryOp::Eq => (PREC_COMPARE, "=="),
        BinaryOp::Ne => (PREC_COMPARE, "!="),
        BinaryOp::Lt => (PREC_COMPARE, "<"),
        BinaryOp::Le => (PREC_COMPARE, "<="),
        BinaryOp::Gt => (PREC_COMPARE, ">"),
        BinaryOp::Ge => (PREC_COMPARE, ">="),
        BinaryOp::Add => (PREC_ADD, "+"),
        BinaryOp::Sub => (PREC_ADD, "-"),
        BinaryOp::Mul => (PREC_MUL, "*"),
        BinaryOp::Div => (PREC_MUL, "/"),
        BinaryOp::Mod => (PREC_MUL, "%"),
    }
}

fn format_float(value: f64) -> String {
    let mut text = value.to_string();
    if !text.contains('.') && !text.contains('e') && !text.contains('E') {
        text.push_str(".0");
    }
    text
}

fn write_indent(out: &mut String, level: usize) {
    for _ in 0..level {
        out.push_str(INDENT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_float_literals_as_float_tokens() {
        let source = "x = 20.0\n";
        let formatted = format_source(source).expect("expected formatter to succeed");
        assert!(formatted.contains("20.0"));
    }
}
