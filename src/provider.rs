use std::collections::VecDeque;
use std::env;
use std::process::Command;

use serde_json::{Map as JsonMap, Value as JsonValue, json};

use crate::error::{SaftError, SaftResult};

const OPENROUTER_CHAT_COMPLETIONS_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const DEFAULT_OPENROUTER_MODEL: &str = "openai/gpt-4o-mini";

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub param_names: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub args: JsonValue,
}

#[derive(Debug, Clone)]
pub struct ToolResult {
    pub id: String,
    pub name: String,
    pub args: JsonValue,
    pub output: JsonValue,
}

#[derive(Debug, Clone)]
pub struct PromptRequest {
    pub prompt: String,
    pub tools: Vec<ToolDefinition>,
    pub tool_results: Vec<ToolResult>,
}

#[derive(Debug, Clone)]
pub enum PromptResponse {
    FinalText(String),
    ToolCalls(Vec<ToolCall>),
}

pub trait PromptProvider {
    fn complete(&mut self, request: PromptRequest) -> SaftResult<PromptResponse>;
}

#[derive(Default)]
pub struct NoopProvider;

impl PromptProvider for NoopProvider {
    fn complete(&mut self, _request: PromptRequest) -> SaftResult<PromptResponse> {
        Err(SaftError::new(
            "no prompt provider configured; use a mock provider for local runs",
        ))
    }
}

pub struct SequenceProvider {
    responses: VecDeque<PromptResponse>,
}

impl SequenceProvider {
    pub fn from_texts(texts: Vec<String>) -> Self {
        Self {
            responses: texts.into_iter().map(PromptResponse::FinalText).collect(),
        }
    }
}

impl PromptProvider for SequenceProvider {
    fn complete(&mut self, _request: PromptRequest) -> SaftResult<PromptResponse> {
        self.responses
            .pop_front()
            .ok_or_else(|| SaftError::new("sequence provider has no more responses"))
    }
}

#[derive(Debug, Clone)]
pub struct OpenRouterConfig {
    pub api_key: String,
    pub model: String,
    pub temperature: f32,
    pub app_name: Option<String>,
    pub referer: Option<String>,
}

pub struct OpenRouterProvider {
    config: OpenRouterConfig,
}

impl OpenRouterProvider {
    pub fn from_env(
        api_key_env: &str,
        model: Option<String>,
        temperature: Option<f32>,
    ) -> SaftResult<Self> {
        let api_key = env::var(api_key_env).map_err(|_| {
            SaftError::new(format!(
                "missing API key in env var '{api_key_env}' for OpenRouter provider"
            ))
        })?;

        let config = OpenRouterConfig {
            api_key,
            model: model.unwrap_or_else(|| DEFAULT_OPENROUTER_MODEL.to_string()),
            temperature: temperature.unwrap_or(0.0),
            app_name: Some("orangensaft".to_string()),
            referer: None,
        };

        Self::new(config)
    }

    pub fn new(config: OpenRouterConfig) -> SaftResult<Self> {
        if config.api_key.trim().is_empty() {
            return Err(SaftError::new("OpenRouter API key is empty"));
        }
        Ok(Self { config })
    }
}

impl PromptProvider for OpenRouterProvider {
    fn complete(&mut self, request: PromptRequest) -> SaftResult<PromptResponse> {
        let mut payload = json!({
            "model": self.config.model,
            "messages": build_openrouter_messages(&request.prompt, &request.tool_results),
            "temperature": self.config.temperature,
        });

        if !request.tools.is_empty() {
            let tools = request
                .tools
                .iter()
                .map(openrouter_tool_definition)
                .collect::<Vec<_>>();
            payload
                .as_object_mut()
                .expect("payload should be object")
                .insert("tools".to_string(), JsonValue::Array(tools));
        }

        let payload_text = serde_json::to_string(&payload).map_err(|err| {
            SaftError::new(format!("failed to serialize OpenRouter payload: {err}"))
        })?;

        let mut cmd = Command::new("curl");
        cmd.arg("-sS")
            .arg("-X")
            .arg("POST")
            .arg(OPENROUTER_CHAT_COMPLETIONS_URL)
            .arg("-H")
            .arg(format!("Authorization: Bearer {}", self.config.api_key))
            .arg("-H")
            .arg("Content-Type: application/json")
            .arg("--data")
            .arg(payload_text);

        if let Some(app_name) = &self.config.app_name {
            cmd.arg("-H").arg(format!("X-Title: {app_name}"));
        }

        if let Some(referer) = &self.config.referer {
            cmd.arg("-H").arg(format!("HTTP-Referer: {referer}"));
        }

        let output = cmd
            .output()
            .map_err(|err| SaftError::new(format!("failed to execute curl: {err}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let message = truncate_for_error(stderr.trim(), 500);
            return Err(SaftError::new(format!(
                "OpenRouter request failed via curl (status {}): {}",
                output.status, message
            )));
        }

        let body_text = String::from_utf8(output.stdout)
            .map_err(|err| SaftError::new(format!("OpenRouter response is not UTF-8: {err}")))?;

        let parsed = serde_json::from_str::<JsonValue>(&body_text)
            .map_err(|err| SaftError::new(format!("invalid OpenRouter JSON response: {err}")))?;

        if let Some(error_obj) = parsed.get("error") {
            return Err(SaftError::new(format!(
                "OpenRouter error: {}",
                truncate_for_error(&error_obj.to_string(), 500)
            )));
        }

        parse_openrouter_response(parsed)
    }
}

#[derive(Default)]
pub struct HeuristicMockProvider;

impl HeuristicMockProvider {
    pub fn new() -> Self {
        Self
    }
}

impl PromptProvider for HeuristicMockProvider {
    fn complete(&mut self, request: PromptRequest) -> SaftResult<PromptResponse> {
        if request.tools.is_empty() {
            return complete_plain_prompt(&request.prompt)
                .map(PromptResponse::FinalText)
                .ok_or_else(|| {
                    SaftError::new("mock provider could not infer a response for this prompt")
                });
        }

        complete_tool_prompt(request)
            .ok_or_else(|| SaftError::new("mock provider could not infer tool-calling behavior"))
    }
}

fn build_openrouter_messages(prompt: &str, tool_results: &[ToolResult]) -> Vec<JsonValue> {
    let mut messages = Vec::new();
    messages.push(json!({
        "role": "user",
        "content": prompt,
    }));

    for result in tool_results {
        let args_json = serde_json::to_string(&result.args).unwrap_or_else(|_| "{}".to_string());
        let output_json =
            serde_json::to_string(&result.output).unwrap_or_else(|_| "null".to_string());

        messages.push(json!({
            "role": "assistant",
            "tool_calls": [{
                "id": result.id,
                "type": "function",
                "function": {
                    "name": result.name,
                    "arguments": args_json,
                }
            }]
        }));

        messages.push(json!({
            "role": "tool",
            "tool_call_id": result.id,
            "name": result.name,
            "content": output_json,
        }));
    }

    messages
}

fn openrouter_tool_definition(tool: &ToolDefinition) -> JsonValue {
    let mut properties = JsonMap::new();
    for param in &tool.param_names {
        properties.insert(param.clone(), JsonValue::Object(JsonMap::new()));
    }

    json!({
        "type": "function",
        "function": {
            "name": tool.name,
            "description": format!("Interpreter function {}", tool.name),
            "parameters": {
                "type": "object",
                "properties": properties,
                "required": tool.param_names,
                "additionalProperties": false,
            }
        }
    })
}

fn parse_openrouter_response(response: JsonValue) -> SaftResult<PromptResponse> {
    let choices = response
        .get("choices")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| SaftError::new("OpenRouter response had no 'choices' array"))?;

    let choice = choices
        .first()
        .ok_or_else(|| SaftError::new("OpenRouter response had an empty 'choices' array"))?;

    let message = choice
        .get("message")
        .ok_or_else(|| SaftError::new("OpenRouter response choice is missing 'message'"))?;

    if let Some(tool_calls) = message.get("tool_calls").and_then(JsonValue::as_array)
        && !tool_calls.is_empty()
    {
        let mut calls = Vec::new();

        for (idx, call) in tool_calls.iter().enumerate() {
            let call_type = call
                .get("type")
                .and_then(JsonValue::as_str)
                .unwrap_or("function");
            if call_type != "function" {
                return Err(SaftError::new(format!(
                    "unsupported tool call type from OpenRouter: {call_type}"
                )));
            }

            let function = call
                .get("function")
                .ok_or_else(|| SaftError::new("tool call missing 'function' object"))?;

            let name = function
                .get("name")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| SaftError::new("tool call function missing 'name'"))?
                .to_string();

            let args_value = function
                .get("arguments")
                .ok_or_else(|| SaftError::new(format!("tool call '{name}' missing arguments")))?;

            let args = match args_value {
                JsonValue::String(text) => {
                    serde_json::from_str::<JsonValue>(text).map_err(|err| {
                        SaftError::new(format!("invalid tool call arguments for '{name}': {err}"))
                    })?
                }
                other => other.clone(),
            };

            let id = call
                .get("id")
                .and_then(JsonValue::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("tool_call_{}", idx + 1));

            calls.push(ToolCall { id, name, args });
        }

        return Ok(PromptResponse::ToolCalls(calls));
    }

    let content = message.get("content").cloned().unwrap_or(JsonValue::Null);
    let text = message_content_to_text(content);
    if text.trim().is_empty() {
        return Err(SaftError::new(
            "OpenRouter returned empty assistant content and no tool calls",
        ));
    }

    Ok(PromptResponse::FinalText(text))
}

fn message_content_to_text(content: JsonValue) -> String {
    match content {
        JsonValue::String(text) => text,
        JsonValue::Array(items) => {
            let mut out = String::new();
            for item in items {
                if let Some(text) = item.get("text").and_then(JsonValue::as_str) {
                    out.push_str(text);
                }
            }
            out
        }
        JsonValue::Null => String::new(),
        other => other.to_string(),
    }
}

fn truncate_for_error(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    text.chars().take(max_chars).collect::<String>() + "..."
}

fn complete_plain_prompt(prompt: &str) -> Option<String> {
    if let Some(sum) = parse_simple_addition(prompt) {
        return Some(sum.to_string());
    }

    if let Some(upper_json) = uppercase_json_array_from_prompt(prompt) {
        return Some(upper_json);
    }

    None
}

fn complete_tool_prompt(request: PromptRequest) -> Option<PromptResponse> {
    let lower = request.prompt.to_ascii_lowercase();

    if lower.contains("if n is even") && lower.contains("if n is odd") {
        return choose_even_odd_calls(request);
    }

    if request.tools.len() == 2 && lower.contains("1) call") && lower.contains("2) call") {
        return chain_two_tools(request);
    }

    if request.tools.len() == 1 && lower.contains("wants to talk to") {
        return single_pair_tool_call(request);
    }

    if request.tools.len() == 1 {
        return map_single_tool(request);
    }

    None
}

fn map_single_tool(request: PromptRequest) -> Option<PromptResponse> {
    let tool = request.tools.first()?.clone();

    if !request.tool_results.is_empty() {
        let outputs = request
            .tool_results
            .iter()
            .map(|r| r.output.clone())
            .collect::<Vec<_>>();
        return serde_json::to_string(&JsonValue::Array(outputs))
            .ok()
            .map(PromptResponse::FinalText);
    }

    let first_array = extract_first_json_array(&request.prompt)?;
    let JsonValue::Array(items) = first_array else {
        return None;
    };

    let mut calls = Vec::with_capacity(items.len());
    for (idx, item) in items.into_iter().enumerate() {
        let args = args_for_item(&tool.param_names, item)?;
        calls.push(ToolCall {
            id: format!("call_{}", idx + 1),
            name: tool.name.clone(),
            args,
        });
    }

    Some(PromptResponse::ToolCalls(calls))
}

fn choose_even_odd_calls(request: PromptRequest) -> Option<PromptResponse> {
    let square = request.tools.first()?.clone();
    let cube = request.tools.get(1)?.clone();

    if !request.tool_results.is_empty() {
        let outputs = request
            .tool_results
            .iter()
            .map(|r| r.output.clone())
            .collect::<Vec<_>>();
        return serde_json::to_string(&JsonValue::Array(outputs))
            .ok()
            .map(PromptResponse::FinalText);
    }

    let first_array = extract_first_json_array(&request.prompt)?;
    let JsonValue::Array(items) = first_array else {
        return None;
    };

    let mut calls = Vec::with_capacity(items.len());
    for (idx, item) in items.into_iter().enumerate() {
        let n = item.as_i64()?;
        let tool = if n % 2 == 0 { &square } else { &cube };
        calls.push(ToolCall {
            id: format!("call_{}", idx + 1),
            name: tool.name.clone(),
            args: args_from_single(&tool.param_names, JsonValue::Number(n.into()))?,
        });
    }

    Some(PromptResponse::ToolCalls(calls))
}

fn chain_two_tools(request: PromptRequest) -> Option<PromptResponse> {
    let first_tool = request.tools.first()?.clone();
    let second_tool = request.tools.get(1)?.clone();

    if request.tool_results.is_empty() {
        let first_array = extract_first_json_array(&request.prompt)?;
        let JsonValue::Array(items) = first_array else {
            return None;
        };

        let mut calls = Vec::with_capacity(items.len());
        for (idx, item) in items.into_iter().enumerate() {
            calls.push(ToolCall {
                id: format!("call_upper_{}", idx + 1),
                name: first_tool.name.clone(),
                args: args_from_single(&first_tool.param_names, item)?,
            });
        }
        return Some(PromptResponse::ToolCalls(calls));
    }

    let first_stage_results = request
        .tool_results
        .iter()
        .filter(|r| r.name == first_tool.name)
        .cloned()
        .collect::<Vec<_>>();

    let second_stage_results = request
        .tool_results
        .iter()
        .filter(|r| r.name == second_tool.name)
        .cloned()
        .collect::<Vec<_>>();

    if second_stage_results.is_empty() {
        let mut calls = Vec::with_capacity(first_stage_results.len());
        for (idx, result) in first_stage_results.into_iter().enumerate() {
            calls.push(ToolCall {
                id: format!("call_suffix_{}", idx + 1),
                name: second_tool.name.clone(),
                args: args_from_single(&second_tool.param_names, result.output)?,
            });
        }
        return Some(PromptResponse::ToolCalls(calls));
    }

    let outputs = second_stage_results
        .iter()
        .map(|r| r.output.clone())
        .collect::<Vec<_>>();
    serde_json::to_string(&JsonValue::Array(outputs))
        .ok()
        .map(PromptResponse::FinalText)
}

fn single_pair_tool_call(request: PromptRequest) -> Option<PromptResponse> {
    let tool = request.tools.first()?.clone();

    if let Some(result) = request.tool_results.iter().find(|r| r.name == tool.name) {
        return serde_json::to_string(&result.output)
            .ok()
            .map(PromptResponse::FinalText);
    }

    let (from, to) = extract_talk_pair(&request.prompt)?;
    if tool.param_names.len() < 2 {
        return None;
    }

    let mut args = JsonMap::new();
    args.insert(tool.param_names[0].clone(), JsonValue::String(from));
    args.insert(tool.param_names[1].clone(), JsonValue::String(to));

    let call = ToolCall {
        id: "call_1".to_string(),
        name: tool.name,
        args: JsonValue::Object(args),
    };

    Some(PromptResponse::ToolCalls(vec![call]))
}

fn args_for_item(param_names: &[String], item: JsonValue) -> Option<JsonValue> {
    if param_names.is_empty() {
        return Some(JsonValue::Object(JsonMap::new()));
    }

    if param_names.len() == 1 {
        return args_from_single(param_names, item);
    }

    match item {
        JsonValue::Array(values) => {
            if values.len() != param_names.len() {
                return None;
            }
            let mut map = JsonMap::new();
            for (name, value) in param_names.iter().cloned().zip(values.into_iter()) {
                map.insert(name, value);
            }
            Some(JsonValue::Object(map))
        }
        JsonValue::Object(obj) => {
            let mut map = JsonMap::new();
            for name in param_names {
                let value = obj.get(name)?.clone();
                map.insert(name.clone(), value);
            }
            Some(JsonValue::Object(map))
        }
        _ => None,
    }
}

fn args_from_single(param_names: &[String], value: JsonValue) -> Option<JsonValue> {
    let name = param_names.first()?.clone();
    let mut map = JsonMap::new();
    map.insert(name, value);
    Some(JsonValue::Object(map))
}

fn extract_talk_pair(prompt: &str) -> Option<(String, String)> {
    let mut normalized = prompt.to_ascii_lowercase();
    for ch in [',', '.', ';', ':', '\n'] {
        normalized = normalized.replace(ch, " ");
    }
    let words = normalized.split_whitespace().collect::<Vec<_>>();

    if words.len() < 6 {
        return None;
    }

    for idx in 0..(words.len() - 5) {
        if words[idx + 1] == "wants"
            && words[idx + 2] == "to"
            && words[idx + 3] == "talk"
            && words[idx + 4] == "to"
        {
            return Some((words[idx].to_string(), words[idx + 5].to_string()));
        }
    }

    None
}

fn parse_simple_addition(prompt: &str) -> Option<i64> {
    for line in prompt.lines() {
        let normalized = line.replace('?', " ").replace(',', " ").replace('=', " ");
        let words = normalized.split_whitespace().collect::<Vec<_>>();

        if words.len() < 3 {
            continue;
        }

        for window in words.windows(3) {
            let Some(a) = window[0].parse::<i64>().ok() else {
                continue;
            };
            if window[1] != "+" {
                continue;
            }
            let Some(b) = window[2].parse::<i64>().ok() else {
                continue;
            };
            return Some(a + b);
        }
    }

    None
}

fn uppercase_json_array_from_prompt(prompt: &str) -> Option<String> {
    let lower = prompt.to_ascii_lowercase();
    if !lower.contains("uppercase") {
        return None;
    }

    let array_value = extract_first_json_array(prompt)?;
    let JsonValue::Array(items) = array_value else {
        return None;
    };

    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let JsonValue::String(text) = item else {
            return None;
        };
        out.push(JsonValue::String(text.to_uppercase()));
    }

    serde_json::to_string(&JsonValue::Array(out)).ok()
}

fn extract_first_json_array(prompt: &str) -> Option<JsonValue> {
    let bytes = prompt.as_bytes();
    let mut start_idx = 0usize;

    while start_idx < bytes.len() {
        if bytes[start_idx] != b'[' {
            start_idx += 1;
            continue;
        }

        let mut depth = 1usize;
        let mut idx = start_idx + 1;
        let mut in_string = false;
        let mut escaped = false;

        while idx < bytes.len() {
            let byte = bytes[idx];

            if in_string {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    in_string = false;
                }
                idx += 1;
                continue;
            }

            match byte {
                b'"' => in_string = true,
                b'[' => depth += 1,
                b']' => {
                    depth -= 1;
                    if depth == 0 {
                        let candidate = &prompt[start_idx..=idx];
                        if let Ok(value) = serde_json::from_str::<JsonValue>(candidate) {
                            return Some(value);
                        }
                        break;
                    }
                }
                _ => {}
            }

            idx += 1;
        }

        start_idx += 1;
    }

    None
}
