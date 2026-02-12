use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use polars::prelude::{AnyValue, ChunkAgg, DataType};
use serde_json::{Map as JsonMap, Value as JsonValue, json};

use crate::ast::{
    BinaryOp, Expr, FnDef, FnParam, Pattern, Program, PromptExpr, PromptPart, SchemaExpr, Stmt,
    UnaryOp,
};
use crate::error::{SaftError, SaftResult, Span};
use crate::provider::{
    HeuristicMockProvider, PromptProvider, PromptRequest, PromptResponse, ToolCall, ToolDefinition,
    ToolResult,
};
use crate::schema;
use crate::stdlib;
use crate::value::{DataFrameValue, FunctionId, Value};

type EnvRef = Rc<RefCell<Env>>;
type BuiltinFn = fn(Vec<Value>) -> SaftResult<Value>;

#[derive(Debug)]
struct Env {
    values: HashMap<String, Value>,
    parent: Option<EnvRef>,
}

impl Env {
    fn new(parent: Option<EnvRef>) -> Self {
        Self {
            values: HashMap::new(),
            parent,
        }
    }
}

#[derive(Clone)]
enum RuntimeFunction {
    User(UserFunction),
    Builtin(BuiltinFunction),
}

#[derive(Clone)]
struct UserFunction {
    name: String,
    params: Vec<FnParam>,
    return_schema: Option<SchemaExpr>,
    body: Vec<Stmt>,
    closure: EnvRef,
}

#[derive(Clone, Copy)]
struct BuiltinFunction {
    name: &'static str,
    arity: usize,
    func: BuiltinFn,
}

pub struct Runtime {
    global: EnvRef,
    functions: Vec<RuntimeFunction>,
    provider: Box<dyn PromptProvider>,
    options: RuntimeOptions,
}

#[derive(Debug, Clone, Copy)]
pub struct RuntimeOptions {
    pub max_tool_rounds: usize,
    pub max_tool_calls: usize,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            max_tool_rounds: 8,
            max_tool_calls: 32,
        }
    }
}

enum Flow {
    Continue,
    Return(Value),
}

impl Runtime {
    pub fn new() -> Self {
        Self::with_provider_and_options(
            Box::new(HeuristicMockProvider::new()),
            RuntimeOptions::default(),
        )
    }

    pub fn with_provider(provider: Box<dyn PromptProvider>) -> Self {
        Self::with_provider_and_options(provider, RuntimeOptions::default())
    }

    pub fn with_provider_and_options(
        provider: Box<dyn PromptProvider>,
        options: RuntimeOptions,
    ) -> Self {
        let global = Rc::new(RefCell::new(Env::new(None)));
        let mut runtime = Self {
            global,
            functions: Vec::new(),
            provider,
            options,
        };
        runtime.install_builtins();
        runtime
    }

    pub fn run_program(&mut self, program: &Program) -> SaftResult<()> {
        let flow = self.exec_block(&program.stmts, self.global.clone())?;
        if let Flow::Return(_) = flow {
            return Err(SaftError::with_span(
                "return statement is only valid inside a function",
                program.span,
            ));
        }
        Ok(())
    }

    fn install_builtins(&mut self) {
        for builtin in stdlib::BUILTINS {
            self.register_builtin(builtin.name, builtin.arity, builtin.func);
        }
    }

    fn register_builtin(&mut self, name: &'static str, arity: usize, func: BuiltinFn) {
        let id = self.functions.len();
        self.functions
            .push(RuntimeFunction::Builtin(BuiltinFunction {
                name,
                arity,
                func,
            }));
        self.global
            .borrow_mut()
            .values
            .insert(name.to_string(), Value::Function(id));
    }

    fn register_user_function(&mut self, def: &FnDef, env: EnvRef) -> FunctionId {
        let id = self.functions.len();
        self.functions.push(RuntimeFunction::User(UserFunction {
            name: def.name.clone(),
            params: def.params.clone(),
            return_schema: def.return_schema.clone(),
            body: def.body.clone(),
            closure: env,
        }));
        id
    }

    fn exec_block(&mut self, stmts: &[Stmt], env: EnvRef) -> SaftResult<Flow> {
        for stmt in stmts {
            match self.exec_stmt(stmt, env.clone())? {
                Flow::Continue => {}
                Flow::Return(value) => return Ok(Flow::Return(value)),
            }
        }
        Ok(Flow::Continue)
    }

    fn exec_stmt(&mut self, stmt: &Stmt, env: EnvRef) -> SaftResult<Flow> {
        match stmt {
            Stmt::FnDef(def) => {
                let id = self.register_user_function(def, env.clone());
                env.borrow_mut()
                    .values
                    .insert(def.name.clone(), Value::Function(id));
                Ok(Flow::Continue)
            }
            Stmt::Assign {
                name,
                annotation,
                value,
                span,
            } => {
                let evaluated = match (annotation, value) {
                    (Some(schema), Expr::Prompt(prompt)) => {
                        self.eval_typed_prompt_assignment(name, prompt, schema, env.clone(), *span)?
                    }
                    _ => {
                        let direct = self.eval_expr(value, env.clone())?;
                        if let Some(schema) = annotation {
                            if let Err(detail) = schema::validate(&direct, schema) {
                                return Err(SaftError::with_span(
                                    format!("schema validation failed for '{name}': {detail}"),
                                    *span,
                                ));
                            }
                        }
                        direct
                    }
                };

                env.borrow_mut().values.insert(name.clone(), evaluated);
                Ok(Flow::Continue)
            }
            Stmt::If {
                cond,
                then_block,
                else_block,
                ..
            } => {
                let cond_value = self.eval_expr(cond, env.clone())?;
                if cond_value.is_truthy() {
                    self.exec_block(then_block, env)
                } else if let Some(block) = else_block {
                    self.exec_block(block, env)
                } else {
                    Ok(Flow::Continue)
                }
            }
            Stmt::For {
                pattern,
                iter,
                body,
                span,
            } => {
                let iter_value = self.eval_expr(iter, env.clone())?;
                let items = match iter_value {
                    Value::List(items) => items,
                    Value::Tuple(items) => items,
                    other => {
                        return Err(SaftError::with_span(
                            format!(
                                "for-loop expects list or tuple iterable, got {}",
                                other.type_name()
                            ),
                            *span,
                        ));
                    }
                };

                for item in items {
                    self.bind_pattern(pattern, item, env.clone(), *span)?;
                    match self.exec_block(body, env.clone())? {
                        Flow::Continue => {}
                        Flow::Return(value) => return Ok(Flow::Return(value)),
                    }
                }

                Ok(Flow::Continue)
            }
            Stmt::Return { value, .. } => {
                let ret_value = if let Some(expr) = value {
                    self.eval_expr(expr, env)?
                } else {
                    Value::Nil
                };
                Ok(Flow::Return(ret_value))
            }
            Stmt::Assert { expr, span } => {
                let value = self.eval_expr(expr, env)?;
                if value.is_truthy() {
                    Ok(Flow::Continue)
                } else {
                    Err(SaftError::with_span(
                        format!("assertion failed: expression evaluated to {value}"),
                        *span,
                    ))
                }
            }
            Stmt::Expr { expr, .. } => {
                self.eval_expr(expr, env)?;
                Ok(Flow::Continue)
            }
        }
    }

    fn bind_pattern(
        &self,
        pattern: &Pattern,
        value: Value,
        env: EnvRef,
        span: Span,
    ) -> SaftResult<()> {
        match pattern {
            Pattern::Name(name) => {
                env.borrow_mut().values.insert(name.clone(), value);
                Ok(())
            }
            Pattern::Tuple(names) => {
                let Value::Tuple(items) = value else {
                    return Err(SaftError::with_span(
                        "tuple destructuring requires tuple values",
                        span,
                    ));
                };

                if items.len() != names.len() {
                    return Err(SaftError::with_span(
                        format!(
                            "tuple destructuring expected {} values, got {}",
                            names.len(),
                            items.len()
                        ),
                        span,
                    ));
                }

                for (name, item) in names.iter().cloned().zip(items.into_iter()) {
                    env.borrow_mut().values.insert(name, item);
                }
                Ok(())
            }
        }
    }

    fn eval_expr(&mut self, expr: &Expr, env: EnvRef) -> SaftResult<Value> {
        match expr {
            Expr::Int(v, _) => Ok(Value::Int(*v)),
            Expr::Float(v, _) => Ok(Value::Float(*v)),
            Expr::Bool(v, _) => Ok(Value::Bool(*v)),
            Expr::Str(v, _) => Ok(Value::String(v.clone())),
            Expr::Nil(_) => Ok(Value::Nil),
            Expr::Var(name, span) => self
                .get_var(env, name)
                .ok_or_else(|| SaftError::with_span(format!("undefined name '{name}'"), *span)),
            Expr::List(items, _) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(self.eval_expr(item, env.clone())?);
                }
                Ok(Value::List(out))
            }
            Expr::Tuple(items, _) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(self.eval_expr(item, env.clone())?);
                }
                Ok(Value::Tuple(out))
            }
            Expr::Object(fields, _) => {
                let mut out = BTreeMap::new();
                for (key, value) in fields {
                    out.insert(key.clone(), self.eval_expr(value, env.clone())?);
                }
                Ok(Value::Object(out))
            }
            Expr::Unary { op, expr, span } => {
                let value = self.eval_expr(expr, env)?;
                match op {
                    UnaryOp::Neg => match value {
                        Value::Int(v) => Ok(Value::Int(-v)),
                        Value::Float(v) => Ok(Value::Float(-v)),
                        other => Err(SaftError::with_span(
                            format!("unary '-' expects number, got {}", other.type_name()),
                            *span,
                        )),
                    },
                    UnaryOp::Not => Ok(Value::Bool(!value.is_truthy())),
                }
            }
            Expr::Binary {
                left,
                op,
                right,
                span,
            } => self.eval_binary(left, op, right, env, *span),
            Expr::Call { callee, args, span } => {
                let callee_value = self.eval_expr(callee, env.clone())?;
                let mut evaluated_args = Vec::with_capacity(args.len());
                for arg in args {
                    evaluated_args.push(self.eval_expr(arg, env.clone())?);
                }

                match callee_value {
                    Value::Function(id) => self.call_function(id, evaluated_args, *span),
                    other => Err(SaftError::with_span(
                        format!(
                            "attempted to call non-function value of type {}",
                            other.type_name()
                        ),
                        *span,
                    )),
                }
            }
            Expr::Index {
                target,
                index,
                span,
            } => {
                let target_value = self.eval_expr(target, env.clone())?;
                let index_value = self.eval_expr(index, env)?;
                self.eval_index(target_value, index_value, *span)
            }
            Expr::Member { target, name, span } => {
                let target_value = self.eval_expr(target, env)?;
                match target_value {
                    Value::Object(map) => map.get(name).cloned().ok_or_else(|| {
                        SaftError::with_span(format!("object has no field '{name}'"), *span)
                    }),
                    other => Err(SaftError::with_span(
                        format!("member access expects object, got {}", other.type_name()),
                        *span,
                    )),
                }
            }
            Expr::TupleIndex {
                target,
                index,
                span,
            } => {
                let target_value = self.eval_expr(target, env)?;
                match target_value {
                    Value::Tuple(items) => items.get(*index).cloned().ok_or_else(|| {
                        SaftError::with_span(format!("tuple index {} out of bounds", index), *span)
                    }),
                    other => Err(SaftError::with_span(
                        format!("tuple index expects tuple, got {}", other.type_name()),
                        *span,
                    )),
                }
            }
            Expr::Prompt(prompt) => {
                let output = self.eval_prompt(prompt, env)?;
                Ok(Value::String(output))
            }
        }
    }

    fn eval_prompt(&mut self, prompt: &PromptExpr, env: EnvRef) -> SaftResult<String> {
        let (rendered_prompt, tools, tool_map) = self.render_prompt(prompt, env)?;
        self.run_prompt_with_tools(&rendered_prompt, &tools, &tool_map, prompt.span)
    }

    fn eval_typed_prompt_assignment(
        &mut self,
        name: &str,
        prompt: &PromptExpr,
        schema_expr: &SchemaExpr,
        env: EnvRef,
        span: Span,
    ) -> SaftResult<Value> {
        let (rendered_prompt, tools, tool_map) = self.render_prompt(prompt, env)?;
        let schema_json = schema::to_json_schema(schema_expr);
        let hardened_prompt = self.build_typed_prompt_contract(
            &rendered_prompt,
            schema_expr,
            &schema_json,
            None,
            None,
        );
        let first_raw = self.run_prompt_with_tools(&hardened_prompt, &tools, &tool_map, span)?;

        match self.parse_and_validate_typed_prompt_output(&first_raw, schema_expr, span) {
            Ok(value) => Ok(value),
            Err(first_error) => {
                let repaired_prompt = self.build_typed_prompt_contract(
                    &rendered_prompt,
                    schema_expr,
                    &schema_json,
                    Some(&first_error),
                    Some(&first_raw),
                );
                let second_raw =
                    self.run_prompt_with_tools(&repaired_prompt, &tools, &tool_map, span)?;

                self.parse_and_validate_typed_prompt_output(&second_raw, schema_expr, span)
                    .map_err(|second_error| {
                        SaftError::with_span(
                            format!(
                                "schema validation failed for '{name}' after repair attempt: first error: {}; second error: {}",
                                first_error, second_error
                            ),
                            span,
                        )
                    })
            }
        }
    }

    fn parse_and_validate_typed_prompt_output(
        &self,
        raw_output: &str,
        schema_expr: &SchemaExpr,
        span: Span,
    ) -> Result<Value, String> {
        let parsed = self
            .parse_json_response(raw_output, span)
            .map_err(|err| err.message)?;
        let normalized = self.unwrap_single_field_wrapper(parsed, schema_expr);
        schema::validate(&normalized, schema_expr).map_err(|detail| {
            format!(
                "expected {}, {}",
                schema::schema_to_string(schema_expr),
                detail
            )
        })?;
        Ok(normalized)
    }

    fn build_typed_prompt_contract(
        &self,
        base_prompt: &str,
        schema_expr: &SchemaExpr,
        schema_json: &JsonValue,
        previous_error: Option<&str>,
        previous_output: Option<&str>,
    ) -> String {
        let mut hardened = String::new();
        hardened.push_str(base_prompt.trim_end());
        hardened.push_str("\n\n---\nOutput contract (mandatory):\n");
        hardened.push_str("- Return ONLY valid JSON.\n");
        hardened.push_str("- Do not include markdown fences.\n");
        hardened.push_str("- Do not include commentary.\n");
        hardened.push_str("- Output must match this JSON Schema exactly:\n");
        hardened.push_str(
            &serde_json::to_string_pretty(schema_json).unwrap_or_else(|_| schema_json.to_string()),
        );
        hardened.push('\n');
        hardened.push_str(&format!(
            "\nTop-level expected type: {}.\n",
            schema::schema_to_string(schema_expr)
        ));
        if let Some(example) = schema_example_json(schema_expr) {
            hardened.push_str("Example valid output JSON shape:\n");
            hardened.push_str(
                &serde_json::to_string_pretty(&example).unwrap_or_else(|_| example.to_string()),
            );
            hardened.push('\n');
        }
        if matches!(
            schema_expr,
            SchemaExpr::String | SchemaExpr::Int | SchemaExpr::Float | SchemaExpr::Bool
        ) {
            hardened.push_str(
                "Important: return the primitive JSON value directly (not wrapped in an object).\n",
            );
        }

        if let Some(error) = previous_error {
            hardened.push_str("\nPrevious output failed validation:\n");
            hardened.push_str(error);
            hardened.push('\n');
        }

        if let Some(output) = previous_output {
            hardened.push_str("\nPrevious output (for correction):\n");
            hardened.push_str(&truncate_text(output, 1000));
            hardened.push('\n');
        }

        hardened.push_str("\nNow return corrected JSON only.\n");
        hardened
    }

    fn unwrap_single_field_wrapper(&self, value: Value, schema_expr: &SchemaExpr) -> Value {
        if let Value::Object(map) = &value
            && map.len() == 1
            && let Some(inner) = map.values().next().cloned()
            && schema::validate(&inner, schema_expr).is_ok()
        {
            return inner;
        }
        value
    }

    fn run_prompt_with_tools(
        &mut self,
        rendered_prompt: &str,
        tools: &[ToolDefinition],
        tool_map: &HashMap<String, FunctionId>,
        span: Span,
    ) -> SaftResult<String> {
        let mut tool_results: Vec<ToolResult> = Vec::new();
        let mut total_tool_calls = 0usize;

        for _round in 0..self.options.max_tool_rounds {
            let request = PromptRequest {
                prompt: rendered_prompt.to_string(),
                tools: tools.to_vec(),
                tool_results: tool_results.clone(),
            };

            match self.provider.complete(request)? {
                PromptResponse::FinalText(text) => return Ok(text),
                PromptResponse::ToolCalls(calls) => {
                    if calls.is_empty() {
                        return Err(SaftError::with_span(
                            "provider returned empty tool call list",
                            span,
                        ));
                    }

                    if tool_map.is_empty() {
                        return Err(SaftError::with_span(
                            "provider attempted tool calls but no tools are exposed in prompt",
                            span,
                        ));
                    }

                    for call in calls {
                        total_tool_calls += 1;
                        if total_tool_calls > self.options.max_tool_calls {
                            return Err(SaftError::with_span(
                                format!(
                                    "tool call limit exceeded (max-tool-calls={})",
                                    self.options.max_tool_calls
                                ),
                                span,
                            ));
                        }

                        let result = self.execute_tool_call(&call, tool_map, span)?;
                        tool_results.push(result);
                    }
                }
            }
        }

        Err(SaftError::with_span(
            format!(
                "tool-call round limit exceeded (max-tool-rounds={})",
                self.options.max_tool_rounds
            ),
            span,
        ))
    }

    fn render_prompt(
        &mut self,
        prompt: &PromptExpr,
        env: EnvRef,
    ) -> SaftResult<(String, Vec<ToolDefinition>, HashMap<String, FunctionId>)> {
        let mut rendered = String::new();
        let mut tools: Vec<ToolDefinition> = Vec::new();
        let mut tool_map: HashMap<String, FunctionId> = HashMap::new();
        let mut generated_counter = 1usize;

        for part in &prompt.parts {
            match part {
                PromptPart::Text(text) => rendered.push_str(text),
                PromptPart::Interpolation(expr) => {
                    let value = self.eval_expr(expr, env.clone())?;
                    match value {
                        Value::Function(function_id) => {
                            let tool_name = if let Expr::Var(name, _) = expr {
                                name.clone()
                            } else {
                                let mut generated = format!("tool_{generated_counter}");
                                generated_counter += 1;
                                while tool_map.contains_key(&generated) {
                                    generated = format!("tool_{generated_counter}");
                                    generated_counter += 1;
                                }
                                generated
                            };

                            if let Some(existing) = tool_map.get(&tool_name) {
                                if *existing != function_id {
                                    return Err(SaftError::with_span(
                                        format!(
                                            "tool name collision for '{}': maps to multiple functions",
                                            tool_name
                                        ),
                                        expr.span(),
                                    ));
                                }
                            } else {
                                let param_names =
                                    self.function_param_names(function_id, expr.span())?;
                                tools.push(ToolDefinition {
                                    name: tool_name.clone(),
                                    param_names,
                                });
                                tool_map.insert(tool_name.clone(), function_id);
                            }

                            rendered.push_str(&tool_name);
                        }
                        other => {
                            let serialized = self.serialize_prompt_value(&other, expr.span())?;
                            rendered.push_str(&serialized);
                        }
                    }
                }
            }
        }

        Ok((rendered, tools, tool_map))
    }

    fn function_param_names(&self, id: FunctionId, span: Span) -> SaftResult<Vec<String>> {
        let function = self
            .functions
            .get(id)
            .ok_or_else(|| SaftError::with_span("unknown function reference", span))?;

        match function {
            RuntimeFunction::User(user) => Ok(user.params.iter().map(|p| p.name.clone()).collect()),
            RuntimeFunction::Builtin(builtin) => {
                Ok((0..builtin.arity).map(|idx| format!("arg{idx}")).collect())
            }
        }
    }

    fn execute_tool_call(
        &mut self,
        call: &ToolCall,
        tool_map: &HashMap<String, FunctionId>,
        span: Span,
    ) -> SaftResult<ToolResult> {
        let function_id = *tool_map.get(&call.name).ok_or_else(|| {
            SaftError::with_span(
                format!("provider requested unknown tool '{}'", call.name),
                span,
            )
        })?;

        let args = self.tool_args_to_values(function_id, &call.args, span)?;
        let output_value = self.call_function(function_id, args, span)?;
        let output_json = self.value_to_json(&output_value, span)?;

        Ok(ToolResult {
            id: call.id.clone(),
            name: call.name.clone(),
            args: call.args.clone(),
            output: output_json,
        })
    }

    fn tool_args_to_values(
        &self,
        function_id: FunctionId,
        args: &JsonValue,
        span: Span,
    ) -> SaftResult<Vec<Value>> {
        let function = self
            .functions
            .get(function_id)
            .ok_or_else(|| SaftError::with_span("unknown function reference", span))?;

        match function {
            RuntimeFunction::User(user) => self.user_tool_args_to_values(user, args, span),
            RuntimeFunction::Builtin(builtin) => {
                self.builtin_tool_args_to_values(*builtin, args, span)
            }
        }
    }

    fn user_tool_args_to_values(
        &self,
        user: &UserFunction,
        args: &JsonValue,
        span: Span,
    ) -> SaftResult<Vec<Value>> {
        let mut values = Vec::with_capacity(user.params.len());
        match args {
            JsonValue::Array(items) => {
                if items.len() != user.params.len() {
                    return Err(SaftError::with_span(
                        format!(
                            "tool '{}' expects {} arguments, got {}",
                            user.name,
                            user.params.len(),
                            items.len()
                        ),
                        span,
                    ));
                }
                for item in items {
                    values.push(self.json_to_value(item.clone(), span)?);
                }
            }
            JsonValue::Object(map) => {
                if map.len() != user.params.len() {
                    return Err(SaftError::with_span(
                        format!(
                            "tool '{}' expects {} named arguments, got {}",
                            user.name,
                            user.params.len(),
                            map.len()
                        ),
                        span,
                    ));
                }
                for param in &user.params {
                    let item = map.get(&param.name).ok_or_else(|| {
                        SaftError::with_span(
                            format!(
                                "tool '{}' missing required argument '{}'",
                                user.name, param.name
                            ),
                            span,
                        )
                    })?;
                    values.push(self.json_to_value(item.clone(), span)?);
                }
            }
            JsonValue::Null if user.params.is_empty() => {}
            _ => {
                return Err(SaftError::with_span(
                    format!("tool '{}' requires object or array args", user.name),
                    span,
                ));
            }
        }

        Ok(values)
    }

    fn builtin_tool_args_to_values(
        &self,
        builtin: BuiltinFunction,
        args: &JsonValue,
        span: Span,
    ) -> SaftResult<Vec<Value>> {
        let mut values = Vec::new();
        match args {
            JsonValue::Array(items) => {
                if items.len() != builtin.arity {
                    return Err(SaftError::with_span(
                        format!(
                            "builtin '{}' expects {} arguments, got {}",
                            builtin.name,
                            builtin.arity,
                            items.len()
                        ),
                        span,
                    ));
                }
                for item in items {
                    values.push(self.json_to_value(item.clone(), span)?);
                }
            }
            JsonValue::Object(map) if builtin.arity == 1 => {
                let value = map.values().next().ok_or_else(|| {
                    SaftError::with_span(
                        format!("builtin '{}' missing argument", builtin.name),
                        span,
                    )
                })?;
                values.push(self.json_to_value(value.clone(), span)?);
            }
            JsonValue::Null if builtin.arity == 0 => {}
            _ => {
                return Err(SaftError::with_span(
                    format!(
                        "builtin '{}' expects {} arguments",
                        builtin.name, builtin.arity
                    ),
                    span,
                ));
            }
        }
        Ok(values)
    }

    fn serialize_prompt_value(&self, value: &Value, span: Span) -> SaftResult<String> {
        let json = self.value_to_json(value, span)?;
        serde_json::to_string(&json).map_err(|err| {
            SaftError::with_span(
                format!("failed to serialize prompt interpolation: {err}"),
                span,
            )
        })
    }

    fn parse_json_response(&self, raw: &str, span: Span) -> SaftResult<Value> {
        let parsed = serde_json::from_str::<JsonValue>(raw.trim()).map_err(|err| {
            SaftError::with_span(format!("prompt output is not valid JSON: {err}"), span)
        })?;

        self.json_to_value(parsed, span)
    }

    fn value_to_json(&self, value: &Value, span: Span) -> SaftResult<JsonValue> {
        match value {
            Value::Int(v) => Ok(JsonValue::Number((*v).into())),
            Value::Float(v) => serde_json::Number::from_f64(*v)
                .map(JsonValue::Number)
                .ok_or_else(|| SaftError::with_span("cannot serialize non-finite float", span)),
            Value::Bool(v) => Ok(JsonValue::Bool(*v)),
            Value::String(v) => Ok(JsonValue::String(v.clone())),
            Value::List(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(self.value_to_json(item, span)?);
                }
                Ok(JsonValue::Array(out))
            }
            Value::Tuple(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(self.value_to_json(item, span)?);
                }
                Ok(JsonValue::Array(out))
            }
            Value::Object(map) => {
                let mut out = serde_json::Map::new();
                for (key, value) in map {
                    out.insert(key.clone(), self.value_to_json(value, span)?);
                }
                Ok(JsonValue::Object(out))
            }
            Value::DataFrame(df) => self.dataframe_to_context_json(df, span),
            Value::Function(_) => Err(SaftError::with_span(
                "function interpolation requires tool-calling (Milestone 3)",
                span,
            )),
            Value::Nil => Ok(JsonValue::Null),
        }
    }

    fn dataframe_to_context_json(
        &self,
        dataframe: &DataFrameValue,
        _span: Span,
    ) -> SaftResult<JsonValue> {
        const SAMPLE_ROW_LIMIT: usize = 8;
        const NUMERIC_PROFILE_LIMIT: usize = 12;

        let frame = dataframe.frame();
        let row_count = frame.height();
        let column_count = frame.width();

        let columns = frame
            .get_columns()
            .iter()
            .map(|column| {
                json!({
                    "name": column.name().to_string(),
                    "dtype": column.dtype().to_string(),
                })
            })
            .collect::<Vec<_>>();

        let sample_rows = self.dataframe_sample_rows_json(dataframe, SAMPLE_ROW_LIMIT)?;
        let (numeric_profile, numeric_column_count) =
            self.dataframe_numeric_profile_json(dataframe, NUMERIC_PROFILE_LIMIT);

        Ok(json!({
            "__kind": "dataframe_context",
            "shape": {
                "rows": row_count,
                "columns": column_count,
            },
            "columns": columns,
            "sample_rows": sample_rows,
            "numeric_profile": numeric_profile,
            "truncation": {
                "sample_rows_truncated": row_count.saturating_sub(SAMPLE_ROW_LIMIT),
                "numeric_columns_truncated": numeric_column_count.saturating_sub(NUMERIC_PROFILE_LIMIT),
            },
            "llm_guidance": "Use numeric_profile for aggregate questions. Use sample_rows for qualitative patterns. If truncation counters are non-zero, the context is intentionally summarized."
        }))
    }

    fn dataframe_sample_rows_json(
        &self,
        dataframe: &DataFrameValue,
        max_rows: usize,
    ) -> SaftResult<Vec<JsonValue>> {
        let frame = dataframe.frame();
        let rows = std::cmp::min(frame.height(), max_rows);
        let mut out = Vec::with_capacity(rows);

        for row_idx in 0..rows {
            let mut row = JsonMap::new();
            for column in frame.get_columns() {
                let value = column.get(row_idx).map_err(|err| {
                    SaftError::new(format!("failed to read dataframe cell: {err}"))
                })?;
                row.insert(column.name().to_string(), anyvalue_to_json_value(value));
            }
            out.push(JsonValue::Object(row));
        }

        Ok(out)
    }

    fn dataframe_numeric_profile_json(
        &self,
        dataframe: &DataFrameValue,
        max_columns: usize,
    ) -> (Vec<JsonValue>, usize) {
        let mut profile = Vec::new();
        let mut numeric_count = 0usize;

        for column in dataframe.frame().get_columns() {
            let casted = match column.cast(&DataType::Float64) {
                Ok(series) => series,
                Err(_) => continue,
            };
            let as_float = match casted.f64() {
                Ok(values) => values,
                Err(_) => continue,
            };

            let non_null_count = as_float.len().saturating_sub(as_float.null_count());
            if non_null_count == 0 {
                continue;
            }

            numeric_count += 1;
            if profile.len() >= max_columns {
                continue;
            }

            let mut column_profile = JsonMap::new();
            column_profile.insert(
                "column".to_string(),
                JsonValue::String(column.name().to_string()),
            );
            column_profile.insert(
                "non_null_count".to_string(),
                JsonValue::Number((non_null_count as u64).into()),
            );
            if let Some(value) = as_float.mean().and_then(serde_json::Number::from_f64) {
                column_profile.insert("mean".to_string(), JsonValue::Number(value));
            }
            if let Some(value) = as_float.min().and_then(serde_json::Number::from_f64) {
                column_profile.insert("min".to_string(), JsonValue::Number(value));
            }
            if let Some(value) = as_float.max().and_then(serde_json::Number::from_f64) {
                column_profile.insert("max".to_string(), JsonValue::Number(value));
            }

            profile.push(JsonValue::Object(column_profile));
        }

        (profile, numeric_count)
    }

    fn json_to_value(&self, json: JsonValue, span: Span) -> SaftResult<Value> {
        match json {
            JsonValue::Null => Ok(Value::Nil),
            JsonValue::Bool(v) => Ok(Value::Bool(v)),
            JsonValue::String(v) => Ok(Value::String(v)),
            JsonValue::Number(n) => {
                if let Some(v) = n.as_i64() {
                    Ok(Value::Int(v))
                } else if let Some(v) = n.as_f64() {
                    Ok(Value::Float(v))
                } else {
                    Err(SaftError::with_span(
                        "unsupported JSON number representation",
                        span,
                    ))
                }
            }
            JsonValue::Array(items) => {
                let mut out = Vec::with_capacity(items.len());
                for item in items {
                    out.push(self.json_to_value(item, span)?);
                }
                Ok(Value::List(out))
            }
            JsonValue::Object(map) => {
                let mut out = BTreeMap::new();
                for (key, value) in map {
                    out.insert(key, self.json_to_value(value, span)?);
                }
                Ok(Value::Object(out))
            }
        }
    }

    fn eval_binary(
        &mut self,
        left: &Expr,
        op: &BinaryOp,
        right: &Expr,
        env: EnvRef,
        span: Span,
    ) -> SaftResult<Value> {
        match op {
            BinaryOp::And => {
                let left_value = self.eval_expr(left, env.clone())?;
                if !left_value.is_truthy() {
                    return Ok(Value::Bool(false));
                }
                let right_value = self.eval_expr(right, env)?;
                Ok(Value::Bool(right_value.is_truthy()))
            }
            BinaryOp::Or => {
                let left_value = self.eval_expr(left, env.clone())?;
                if left_value.is_truthy() {
                    return Ok(Value::Bool(true));
                }
                let right_value = self.eval_expr(right, env)?;
                Ok(Value::Bool(right_value.is_truthy()))
            }
            _ => {
                let left_value = self.eval_expr(left, env.clone())?;
                let right_value = self.eval_expr(right, env)?;
                self.eval_binary_values(op, left_value, right_value, span)
            }
        }
    }

    fn eval_binary_values(
        &self,
        op: &BinaryOp,
        left: Value,
        right: Value,
        span: Span,
    ) -> SaftResult<Value> {
        match op {
            BinaryOp::Add => match (left, right) {
                (Value::Int(a), Value::Int(b)) => Ok(Value::Int(a + b)),
                (Value::Float(a), Value::Float(b)) => Ok(Value::Float(a + b)),
                (Value::Int(a), Value::Float(b)) => Ok(Value::Float(a as f64 + b)),
                (Value::Float(a), Value::Int(b)) => Ok(Value::Float(a + b as f64)),
                (Value::String(a), Value::String(b)) => Ok(Value::String(a + b.as_str())),
                (a, b) => Err(SaftError::with_span(
                    format!(
                        "operator '+' expects numeric operands or strings, got {} and {}",
                        a.type_name(),
                        b.type_name()
                    ),
                    span,
                )),
            },
            BinaryOp::Sub => self.numeric_binary(left, right, span, |a, b| a - b, |a, b| a - b),
            BinaryOp::Mul => self.numeric_binary(left, right, span, |a, b| a * b, |a, b| a * b),
            BinaryOp::Div => {
                let (a, b) = self.as_f64_pair(left, right, span, "'/'")?;
                if b == 0.0 {
                    return Err(SaftError::with_span("division by zero", span));
                }
                Ok(Value::Float(a / b))
            }
            BinaryOp::Mod => match (left, right) {
                (Value::Int(a), Value::Int(b)) => {
                    if b == 0 {
                        return Err(SaftError::with_span("modulo by zero", span));
                    }
                    Ok(Value::Int(a % b))
                }
                (a, b) => Err(SaftError::with_span(
                    format!(
                        "operator '%' expects integer operands, got {} and {}",
                        a.type_name(),
                        b.type_name()
                    ),
                    span,
                )),
            },
            BinaryOp::Eq => Ok(Value::Bool(left == right)),
            BinaryOp::Ne => Ok(Value::Bool(left != right)),
            BinaryOp::Lt => self.comparison(left, right, span, "<", |a, b| a < b),
            BinaryOp::Le => self.comparison(left, right, span, "<=", |a, b| a <= b),
            BinaryOp::Gt => self.comparison(left, right, span, ">", |a, b| a > b),
            BinaryOp::Ge => self.comparison(left, right, span, ">=", |a, b| a >= b),
            BinaryOp::And | BinaryOp::Or => unreachable!("logical ops are handled earlier"),
        }
    }

    fn numeric_binary(
        &self,
        left: Value,
        right: Value,
        span: Span,
        int_op: fn(i64, i64) -> i64,
        float_op: fn(f64, f64) -> f64,
    ) -> SaftResult<Value> {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => Ok(Value::Int(int_op(a, b))),
            (Value::Float(a), Value::Float(b)) => Ok(Value::Float(float_op(a, b))),
            (Value::Int(a), Value::Float(b)) => Ok(Value::Float(float_op(a as f64, b))),
            (Value::Float(a), Value::Int(b)) => Ok(Value::Float(float_op(a, b as f64))),
            (a, b) => Err(SaftError::with_span(
                format!(
                    "numeric operator expects numbers, got {} and {}",
                    a.type_name(),
                    b.type_name()
                ),
                span,
            )),
        }
    }

    fn comparison(
        &self,
        left: Value,
        right: Value,
        span: Span,
        op_name: &str,
        cmp: fn(f64, f64) -> bool,
    ) -> SaftResult<Value> {
        let (a, b) = self.as_f64_pair(left, right, span, op_name)?;
        Ok(Value::Bool(cmp(a, b)))
    }

    fn as_f64_pair(
        &self,
        left: Value,
        right: Value,
        span: Span,
        op_name: &str,
    ) -> SaftResult<(f64, f64)> {
        let a = match left {
            Value::Int(v) => v as f64,
            Value::Float(v) => v,
            other => {
                return Err(SaftError::with_span(
                    format!(
                        "operator {op_name} expects numeric operands, got {}",
                        other.type_name()
                    ),
                    span,
                ));
            }
        };

        let b = match right {
            Value::Int(v) => v as f64,
            Value::Float(v) => v,
            other => {
                return Err(SaftError::with_span(
                    format!(
                        "operator {op_name} expects numeric operands, got {}",
                        other.type_name()
                    ),
                    span,
                ));
            }
        };

        Ok((a, b))
    }

    fn eval_index(&self, target: Value, index: Value, span: Span) -> SaftResult<Value> {
        match target {
            Value::List(items) => {
                let idx = self.to_index(index, span)?;
                items.get(idx).cloned().ok_or_else(|| {
                    SaftError::with_span(format!("list index {idx} out of bounds"), span)
                })
            }
            Value::Tuple(items) => {
                let idx = self.to_index(index, span)?;
                items.get(idx).cloned().ok_or_else(|| {
                    SaftError::with_span(format!("tuple index {idx} out of bounds"), span)
                })
            }
            Value::Object(map) => {
                let Value::String(key) = index else {
                    return Err(SaftError::with_span(
                        "object index expects string key",
                        span,
                    ));
                };
                map.get(&key)
                    .cloned()
                    .ok_or_else(|| SaftError::with_span(format!("missing key '{key}'"), span))
            }
            other => Err(SaftError::with_span(
                format!("indexing is not supported on {}", other.type_name()),
                span,
            )),
        }
    }

    fn to_index(&self, value: Value, span: Span) -> SaftResult<usize> {
        match value {
            Value::Int(v) if v >= 0 => Ok(v as usize),
            Value::Int(_) => Err(SaftError::with_span("index must be non-negative", span)),
            other => Err(SaftError::with_span(
                format!("index must be int, got {}", other.type_name()),
                span,
            )),
        }
    }

    fn call_function(
        &mut self,
        id: FunctionId,
        args: Vec<Value>,
        call_span: Span,
    ) -> SaftResult<Value> {
        let function = self
            .functions
            .get(id)
            .cloned()
            .ok_or_else(|| SaftError::with_span("unknown function reference", call_span))?;

        match function {
            RuntimeFunction::Builtin(builtin) => {
                if args.len() != builtin.arity {
                    return Err(SaftError::with_span(
                        format!(
                            "builtin '{}' expects {} arguments, got {}",
                            builtin.name,
                            builtin.arity,
                            args.len()
                        ),
                        call_span,
                    ));
                }
                (builtin.func)(args)
            }
            RuntimeFunction::User(user) => {
                if args.len() != user.params.len() {
                    return Err(SaftError::with_span(
                        format!(
                            "function '{}' expects {} arguments, got {}",
                            user.name,
                            user.params.len(),
                            args.len()
                        ),
                        call_span,
                    ));
                }

                let call_env = Rc::new(RefCell::new(Env::new(Some(user.closure.clone()))));
                for (arg, param) in args.into_iter().zip(user.params.iter()) {
                    if let Some(schema) = &param.schema {
                        if let Err(detail) = schema::validate(&arg, schema) {
                            return Err(SaftError::with_span(
                                format!(
                                    "invalid argument for parameter '{}' in '{}': {}",
                                    param.name, user.name, detail
                                ),
                                call_span,
                            ));
                        }
                    }
                    call_env.borrow_mut().values.insert(param.name.clone(), arg);
                }

                let flow = self.exec_block(&user.body, call_env)?;
                let result = match flow {
                    Flow::Continue => Value::Nil,
                    Flow::Return(value) => value,
                };

                if let Some(schema) = &user.return_schema {
                    if let Err(detail) = schema::validate(&result, schema) {
                        return Err(SaftError::with_span(
                            format!(
                                "function '{}' returned invalid value for schema {}: {}",
                                user.name,
                                schema::schema_to_string(schema),
                                detail
                            ),
                            call_span,
                        ));
                    }
                }

                Ok(result)
            }
        }
    }

    fn get_var(&self, env: EnvRef, name: &str) -> Option<Value> {
        let mut current = Some(env);
        while let Some(scope) = current {
            if let Some(value) = scope.borrow().values.get(name) {
                return Some(value.clone());
            }
            current = scope.borrow().parent.clone();
        }
        None
    }
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect::<String>() + "..."
}

fn schema_example_json(schema: &SchemaExpr) -> Option<JsonValue> {
    match schema {
        SchemaExpr::Any => None,
        SchemaExpr::Int => Some(JsonValue::Number(1.into())),
        SchemaExpr::Float => serde_json::Number::from_f64(1.5).map(JsonValue::Number),
        SchemaExpr::Bool => Some(JsonValue::Bool(true)),
        SchemaExpr::String => Some(JsonValue::String("example".to_string())),
        SchemaExpr::List(inner) => schema_example_json(inner)
            .map(|v| JsonValue::Array(vec![v]))
            .or_else(|| Some(JsonValue::Array(vec![]))),
        SchemaExpr::Tuple(items) => Some(JsonValue::Array(
            items
                .iter()
                .map(|item| schema_example_json(item).unwrap_or(JsonValue::Null))
                .collect(),
        )),
        SchemaExpr::Object(fields) => {
            let mut obj = serde_json::Map::new();
            for field in fields {
                let example = schema_example_json(&field.schema).unwrap_or(JsonValue::Null);
                obj.insert(field.name.clone(), example);
            }
            Some(JsonValue::Object(obj))
        }
        SchemaExpr::Union(variants) => variants.first().and_then(schema_example_json),
        SchemaExpr::Optional(inner) => schema_example_json(inner).or(Some(JsonValue::Null)),
    }
}

fn anyvalue_to_json_value(value: AnyValue<'_>) -> JsonValue {
    match value {
        AnyValue::Null => JsonValue::Null,
        AnyValue::Boolean(v) => JsonValue::Bool(v),
        AnyValue::Int8(v) => JsonValue::Number((v as i64).into()),
        AnyValue::Int16(v) => JsonValue::Number((v as i64).into()),
        AnyValue::Int32(v) => JsonValue::Number((v as i64).into()),
        AnyValue::Int64(v) => JsonValue::Number(v.into()),
        AnyValue::UInt8(v) => JsonValue::Number((v as u64).into()),
        AnyValue::UInt16(v) => JsonValue::Number((v as u64).into()),
        AnyValue::UInt32(v) => JsonValue::Number((v as u64).into()),
        AnyValue::UInt64(v) => JsonValue::Number(v.into()),
        AnyValue::Float32(v) => serde_json::Number::from_f64(v as f64)
            .map(JsonValue::Number)
            .unwrap_or_else(|| JsonValue::String(v.to_string())),
        AnyValue::Float64(v) => serde_json::Number::from_f64(v)
            .map(JsonValue::Number)
            .unwrap_or_else(|| JsonValue::String(v.to_string())),
        AnyValue::String(v) => JsonValue::String(v.to_string()),
        AnyValue::StringOwned(v) => JsonValue::String(v.to_string()),
        other => JsonValue::String(other.to_string()),
    }
}
