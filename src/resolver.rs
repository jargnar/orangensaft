use std::collections::HashSet;

use crate::ast::{Expr, FnDef, Pattern, Program, PromptPart, Stmt};
use crate::error::{SaftError, SaftResult};

pub fn resolve(program: &Program, builtins: &[&str]) -> SaftResult<()> {
    let resolver = Resolver {
        builtins: builtins.iter().map(|name| (*name).to_string()).collect(),
    };

    let root = HashSet::new();
    resolver.resolve_block(&program.stmts, &root)
}

struct Resolver {
    builtins: HashSet<String>,
}

impl Resolver {
    fn resolve_block(&self, stmts: &[Stmt], parent_scope: &HashSet<String>) -> SaftResult<()> {
        let mut scope = parent_scope.clone();
        let mut fn_names = HashSet::new();

        for stmt in stmts {
            match stmt {
                Stmt::FnDef(FnDef { name, span, .. }) => {
                    if !fn_names.insert(name.clone()) {
                        return Err(SaftError::with_span(
                            format!("duplicate function '{name}' in same scope"),
                            *span,
                        ));
                    }
                    scope.insert(name.clone());
                }
                Stmt::Assign { name, .. } => {
                    scope.insert(name.clone());
                }
                Stmt::For { pattern, .. } => {
                    self.insert_pattern_names(pattern, &mut scope);
                }
                _ => {}
            }
        }

        for stmt in stmts {
            self.resolve_stmt(stmt, &scope)?;
        }

        Ok(())
    }

    fn resolve_stmt(&self, stmt: &Stmt, scope: &HashSet<String>) -> SaftResult<()> {
        match stmt {
            Stmt::FnDef(def) => {
                let mut fn_scope = scope.clone();
                let mut seen_params = HashSet::new();
                for param in &def.params {
                    if !seen_params.insert(param.name.clone()) {
                        return Err(SaftError::with_span(
                            format!(
                                "duplicate parameter '{}' in function '{}'",
                                param.name, def.name
                            ),
                            param.span,
                        ));
                    }
                    fn_scope.insert(param.name.clone());
                }
                self.resolve_block(&def.body, &fn_scope)
            }
            Stmt::Assign { value, .. } => self.resolve_expr(value, scope),
            Stmt::If {
                cond,
                then_block,
                else_block,
                ..
            } => {
                self.resolve_expr(cond, scope)?;
                self.resolve_block(then_block, scope)?;
                if let Some(block) = else_block {
                    self.resolve_block(block, scope)?;
                }
                Ok(())
            }
            Stmt::For {
                pattern,
                iter,
                body,
                ..
            } => {
                self.resolve_expr(iter, scope)?;
                let mut loop_scope = scope.clone();
                self.insert_pattern_names(pattern, &mut loop_scope);
                self.resolve_block(body, &loop_scope)
            }
            Stmt::Return { value, .. } => {
                if let Some(expr) = value {
                    self.resolve_expr(expr, scope)?;
                }
                Ok(())
            }
            Stmt::Assert { expr, .. } | Stmt::Expr { expr, .. } => self.resolve_expr(expr, scope),
        }
    }

    fn resolve_expr(&self, expr: &Expr, scope: &HashSet<String>) -> SaftResult<()> {
        match expr {
            Expr::Var(name, span) => {
                if scope.contains(name) || self.builtins.contains(name) {
                    Ok(())
                } else {
                    Err(SaftError::with_span(
                        format!("undefined name '{name}'"),
                        *span,
                    ))
                }
            }
            Expr::List(items, _) | Expr::Tuple(items, _) => {
                for item in items {
                    self.resolve_expr(item, scope)?;
                }
                Ok(())
            }
            Expr::Object(fields, _) => {
                for (_, value) in fields {
                    self.resolve_expr(value, scope)?;
                }
                Ok(())
            }
            Expr::Unary { expr, .. } => self.resolve_expr(expr, scope),
            Expr::Binary { left, right, .. } => {
                self.resolve_expr(left, scope)?;
                self.resolve_expr(right, scope)
            }
            Expr::Call { callee, args, .. } => {
                self.resolve_expr(callee, scope)?;
                for arg in args {
                    self.resolve_expr(arg, scope)?;
                }
                Ok(())
            }
            Expr::Index { target, index, .. } => {
                self.resolve_expr(target, scope)?;
                self.resolve_expr(index, scope)
            }
            Expr::Member { target, .. } | Expr::TupleIndex { target, .. } => {
                self.resolve_expr(target, scope)
            }
            Expr::Prompt(prompt) => {
                for part in &prompt.parts {
                    if let PromptPart::Interpolation(expr) = part {
                        self.resolve_expr(expr, scope)?;
                    }
                }
                Ok(())
            }
            Expr::Int(_, _)
            | Expr::Float(_, _)
            | Expr::Bool(_, _)
            | Expr::Str(_, _)
            | Expr::Nil(_) => Ok(()),
        }
    }

    fn insert_pattern_names(&self, pattern: &Pattern, scope: &mut HashSet<String>) {
        match pattern {
            Pattern::Name(name) => {
                scope.insert(name.clone());
            }
            Pattern::Tuple(names) => {
                for name in names {
                    scope.insert(name.clone());
                }
            }
        }
    }
}
