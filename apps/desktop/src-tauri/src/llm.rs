use serde::{Deserialize, Serialize};

/// Configuration for an OpenAI-compatible LLM endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub endpoint: Option<String>,
    pub api_key: String,
    pub model: Option<String>,
}

/// A single chat message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: serde_json::Value,
}

/// Image content for vision requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageContent {
    pub base64_data: String,
    pub media_type: String,
}

/// A tool definition sent to the LLM for function calling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// A tool call returned by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub call_type: Option<String>,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: String,
}

/// Normalize an endpoint URL to always end with `/chat/completions`.
fn normalize_endpoint(endpoint: &str) -> String {
    let url = endpoint.trim().trim_end_matches('/');
    if url.is_empty() {
        return "https://api.openai.com/v1/chat/completions".to_string();
    }
    if url.to_lowercase().ends_with("/chat/completions") {
        return url.to_string();
    }
    if url.to_lowercase().ends_with("/v1") {
        return format!("{}/chat/completions", url);
    }
    format!("{}/v1/chat/completions", url)
}

fn resolve_config(config: &LlmConfig) -> (String, String, String) {
    let endpoint = normalize_endpoint(
        config
            .endpoint
            .as_deref()
            .unwrap_or("https://api.openai.com/v1/chat/completions"),
    );
    let model = config
        .model
        .as_deref()
        .unwrap_or("gpt-4o-mini")
        .to_string();
    let api_key = config.api_key.clone();
    (endpoint, api_key, model)
}

/// Call an OpenAI-compatible chat/completions endpoint.
///
/// Returns the assistant message content as a string.
pub async fn call_llm(
    config: &LlmConfig,
    messages: Vec<Message>,
    temperature: f64,
) -> Result<String, String> {
    let (endpoint, api_key, model) = resolve_config(config);
    if api_key.is_empty() {
        return Err("LLM API key is not set".to_string());
    }

    let body = serde_json::json!({
        "model": model,
        "messages": messages,
        "temperature": temperature,
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(&endpoint)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("LLM request failed: {}", e))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read LLM response: {}", e))?;

    if !status.is_success() {
        return Err(format!("LLM returned status {}: {}", status, text));
    }

    let data: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Failed to parse LLM response: {}", e))?;

    let content = data["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .to_string();

    Ok(content)
}

/// Call an OpenAI-compatible chat/completions endpoint with tool definitions.
///
/// Returns the full assistant message (may contain tool_calls).
pub async fn call_llm_with_tools(
    config: &LlmConfig,
    messages: Vec<Message>,
    tools: &[ToolDefinition],
    temperature: f64,
) -> Result<serde_json::Value, String> {
    let (endpoint, api_key, model) = resolve_config(config);
    if api_key.is_empty() {
        return Err("LLM API key is not set".to_string());
    }

    let mut body = serde_json::json!({
        "model": model,
        "messages": messages,
        "temperature": temperature,
    });

    if !tools.is_empty() {
        body["tools"] =
            serde_json::to_value(tools).map_err(|e| format!("Failed to serialize tools: {}", e))?;
    }

    let client = reqwest::Client::new();
    let resp = client
        .post(&endpoint)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("LLM request failed: {}", e))?;

    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("Failed to read LLM response: {}", e))?;

    if !status.is_success() {
        return Err(format!("LLM returned status {}: {}", status, text));
    }

    let data: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("Failed to parse LLM response: {}", e))?;

    let message = data["choices"][0]["message"].clone();
    Ok(message)
}

/// Call an OpenAI-compatible vision endpoint with base64 image(s).
pub async fn call_vision_llm(
    config: &LlmConfig,
    system_prompt: &str,
    text_prompt: &str,
    images: &[ImageContent],
) -> Result<String, String> {
    let mut user_content: Vec<serde_json::Value> = Vec::new();

    // Add text part
    user_content.push(serde_json::json!({
        "type": "text",
        "text": text_prompt,
    }));

    // Add image parts
    for img in images {
        user_content.push(serde_json::json!({
            "type": "image_url",
            "image_url": {
                "url": format!("data:{};base64,{}", img.media_type, img.base64_data),
            }
        }));
    }

    let messages = vec![
        Message {
            role: "system".to_string(),
            content: serde_json::Value::String(system_prompt.to_string()),
        },
        Message {
            role: "user".to_string(),
            content: serde_json::Value::Array(user_content),
        },
    ];

    call_llm(config, messages, 0.2).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_endpoint_default() {
        assert_eq!(
            normalize_endpoint(""),
            "https://api.openai.com/v1/chat/completions"
        );
    }

    #[test]
    fn test_normalize_endpoint_already_complete() {
        assert_eq!(
            normalize_endpoint("https://my.api/v1/chat/completions"),
            "https://my.api/v1/chat/completions"
        );
    }

    #[test]
    fn test_normalize_endpoint_v1_only() {
        assert_eq!(
            normalize_endpoint("https://my.api/v1"),
            "https://my.api/v1/chat/completions"
        );
    }

    #[test]
    fn test_normalize_endpoint_bare() {
        assert_eq!(
            normalize_endpoint("https://my.api"),
            "https://my.api/v1/chat/completions"
        );
    }

    #[test]
    fn test_normalize_endpoint_trailing_slash() {
        assert_eq!(
            normalize_endpoint("https://my.api/v1/"),
            "https://my.api/v1/chat/completions"
        );
    }
}
