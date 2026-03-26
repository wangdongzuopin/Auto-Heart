use crate::settings::AppSettings;
use reqwest::Client;
use serde::{Deserialize, Serialize};

/// 模型路由器 — 统一 OpenAI-compatible + Anthropic API 封装
///
/// 支持提供商（技术文档 §4 + 用户需求）：
/// - Kimi (月之暗面)    https://api.moonshot.cn/v1
/// - Qwen (通义千问)    https://dashscope.aliyuncs.com/compatible-mode/v1
/// - MiniMax            https://api.minimax.chat/v1
/// - GPT (OpenAI)       https://api.openai.com/v1
/// - Claude (Anthropic) https://api.anthropic.com — 原生 Messages API
/// - DeepSeek           https://api.deepseek.com/v1
/// - OpenRouter         https://openrouter.ai/api/v1 (统一代理)
/// - Ollama             http://localhost:11434/v1 (本地)

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    pub provider: String, // "kimi" | "qwen" | "minimax" | "gpt" | "claude" | "deepseek" | "openrouter" | "ollama"
    pub model: String,
    pub api_key: String,
    pub base_url: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct OaiMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
struct OaiRequest {
    model: String,
    messages: Vec<OaiMessage>,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
}

#[derive(Debug, Deserialize)]
struct OaiResponse {
    choices: Vec<OaiChoice>,
}

#[derive(Debug, Deserialize)]
struct OaiChoice {
    message: OaiMessageResp,
}

#[derive(Debug, Deserialize)]
struct OaiMessageResp {
    content: String,
}

// ── Anthropic Messages API ──

#[derive(Debug, Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<OaiMessage>,
}

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
}

#[derive(Debug, Deserialize)]
struct AnthropicContent {
    text: String,
}

/// 路由策略
#[derive(Debug, Clone)]
pub enum TaskType {
    SemanticUnderstanding,
    DeepReasoning,
    Generation,
}

pub struct ModelRouter {
    client: Client,
    configs: std::collections::HashMap<String, ModelConfig>,
}

impl ModelRouter {
    pub fn new() -> Self {
        Self { client: Client::new(), configs: std::collections::HashMap::new() }
    }

    pub fn register_model(&mut self, name: &str, config: ModelConfig) {
        self.configs.insert(name.to_string(), config);
    }

    /// 使用已注册的路由策略调用
    pub async fn route_and_call(
        &self,
        task_type: TaskType,
        prompt: &str,
        system_prompt: Option<&str>,
    ) -> Result<String, String> {
        let model_name = match task_type {
            TaskType::SemanticUnderstanding => {
                if self.configs.contains_key("kimi") { "kimi" }
                else if self.configs.contains_key("qwen") { "qwen" }
                else if self.configs.contains_key("minimax") { "minimax" }
                else if self.configs.contains_key("gpt") { "gpt" }
                else if self.configs.contains_key("ollama") { "ollama" }
                else { return Err("No model configured for semantic understanding".into()); }
            }
            TaskType::DeepReasoning | TaskType::Generation => {
                if self.configs.contains_key("claude") { "claude" }
                else if self.configs.contains_key("gpt") { "gpt" }
                else if self.configs.contains_key("deepseek") { "deepseek" }
                else { return Err("No model configured for deep reasoning".into()); }
            }
        };
        let config = self.configs.get(model_name)
            .ok_or_else(|| format!("Model '{}' not found", model_name))?;
        self.call_model(config, prompt, system_prompt).await
    }

    /// 直接用指定 ModelConfig 调用（不经路由策略）
    pub async fn call_with_config(
        &self,
        config: &ModelConfig,
        prompt: &str,
        system_prompt: Option<&str>,
    ) -> Result<String, String> {
        self.call_model(config, prompt, system_prompt).await
    }

    async fn call_model(
        &self,
        config: &ModelConfig,
        prompt: &str,
        system_prompt: Option<&str>,
    ) -> Result<String, String> {
        if config.provider == "claude" {
            self.call_anthropic(config, prompt, system_prompt).await
        } else {
            self.call_openai_compatible(config, prompt, system_prompt).await
        }
    }

    /// OpenAI-compatible API（Kimi / Qwen / MiniMax / GPT / DeepSeek / OpenRouter / Ollama）
    async fn call_openai_compatible(
        &self,
        config: &ModelConfig,
        prompt: &str,
        system_prompt: Option<&str>,
    ) -> Result<String, String> {
        let mut messages = Vec::new();
        if let Some(sys) = system_prompt {
            messages.push(OaiMessage { role: "system".into(), content: sys.into() });
        }
        messages.push(OaiMessage { role: "user".into(), content: prompt.into() });

        let request = OaiRequest {
            model: config.model.clone(),
            messages,
            max_tokens: Some(2000),
            temperature: Some(0.3),
        };
        let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));

        let mut req = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", config.api_key))
            .header("Content-Type", "application/json")
            .json(&request);

        // OpenRouter 需要额外 header
        if config.provider == "openrouter" {
            req = req
                .header("HTTP-Referer", "https://auto-heart.app")
                .header("X-Title", "Auto-Heart");
        }

        let response = req.send().await.map_err(|e| format!("HTTP error: {}", e))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("API error {}: {}", status, body));
        }

        let resp: OaiResponse = response.json().await
            .map_err(|e| format!("Parse error: {}", e))?;
        resp.choices.first()
            .map(|c| c.message.content.clone())
            .ok_or_else(|| "No response from model".into())
    }

    /// Anthropic Messages API（Claude 原生格式）
    async fn call_anthropic(
        &self,
        config: &ModelConfig,
        prompt: &str,
        system_prompt: Option<&str>,
    ) -> Result<String, String> {
        let request = AnthropicRequest {
            model: config.model.clone(),
            max_tokens: 2000,
            system: system_prompt.map(|s| s.to_string()),
            messages: vec![OaiMessage { role: "user".into(), content: prompt.into() }],
        };

        let url = format!("{}/messages", config.base_url.trim_end_matches('/'));
        let response = self.client
            .post(&url)
            .header("x-api-key", &config.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Anthropic API error {}: {}", status, body));
        }

        let resp: AnthropicResponse = response.json().await
            .map_err(|e| format!("Parse Anthropic response: {}", e))?;
        resp.content.first()
            .map(|c| c.text.clone())
            .ok_or_else(|| "No content in Claude response".into())
    }
}

// ──────────────────────────────────────────────
// 从 AppSettings 动态构建 ModelConfig
// ──────────────────────────────────────────────

/// 根据设置中的 provider 和 model_name 构建 ModelConfig
///
/// 返回 None 表示该提供商的 API Key 未配置（Ollama 除外）
pub fn build_model_config(
    provider: &str,
    model_name: &str,
    settings: &AppSettings,
) -> Option<ModelConfig> {
    let effective_model = if model_name.is_empty() {
        default_model_for_provider(provider).to_string()
    } else {
        model_name.to_string()
    };

    match provider {
        "kimi" => {
            if settings.kimi_api_key.is_empty() { return None; }
            Some(ModelConfig {
                provider: "kimi".to_string(),
                model: effective_model,
                api_key: settings.kimi_api_key.clone(),
                base_url: default_base_url("kimi").to_string(),
            })
        }
        "qwen" => {
            if settings.qwen_api_key.is_empty() { return None; }
            Some(ModelConfig {
                provider: "qwen".to_string(),
                model: effective_model,
                api_key: settings.qwen_api_key.clone(),
                base_url: default_base_url("qwen").to_string(),
            })
        }
        "minimax" => {
            if settings.minimax_api_key.is_empty() { return None; }
            Some(ModelConfig {
                provider: "minimax".to_string(),
                model: effective_model,
                api_key: settings.minimax_api_key.clone(),
                base_url: default_base_url("minimax").to_string(),
            })
        }
        "gpt" => {
            if settings.gpt_api_key.is_empty() { return None; }
            Some(ModelConfig {
                provider: "gpt".to_string(),
                model: effective_model,
                api_key: settings.gpt_api_key.clone(),
                base_url: default_base_url("openai").to_string(),
            })
        }
        "claude" => {
            if settings.claude_api_key.is_empty() { return None; }
            Some(ModelConfig {
                provider: "claude".to_string(),
                model: effective_model,
                api_key: settings.claude_api_key.clone(),
                base_url: default_base_url("anthropic").to_string(),
            })
        }
        "deepseek" => {
            if settings.deepseek_api_key.is_empty() { return None; }
            Some(ModelConfig {
                provider: "deepseek".to_string(),
                model: effective_model,
                api_key: settings.deepseek_api_key.clone(),
                base_url: default_base_url("deepseek").to_string(),
            })
        }
        "openrouter" => {
            if settings.openrouter_api_key.is_empty() { return None; }
            Some(ModelConfig {
                provider: "openrouter".to_string(),
                model: effective_model,
                api_key: settings.openrouter_api_key.clone(),
                base_url: default_base_url("openrouter").to_string(),
            })
        }
        "ollama" => {
            let base = if settings.ollama_base_url.is_empty() {
                "http://localhost:11434".to_string()
            } else {
                settings.ollama_base_url.trim_end_matches('/').trim_end_matches("/v1").to_string()
            };
            Some(ModelConfig {
                provider: "ollama".to_string(),
                model: effective_model,
                api_key: "ollama".to_string(),
                base_url: format!("{}/v1", base),
            })
        }
        _ => None,
    }
}

/// 每个提供商的默认模型名
pub fn default_model_for_provider(provider: &str) -> &'static str {
    match provider {
        "kimi"       => "moonshot-v1-8k",
        "qwen"       => "qwen-plus",
        "minimax"    => "abab6.5s-chat",
        "gpt"        => "gpt-4o-mini",
        "claude"     => "claude-sonnet-4-5",
        "deepseek"   => "deepseek-chat",
        "openrouter" => "openai/gpt-4o-mini",
        "ollama"     => "qwen2.5:7b",
        _            => "unknown",
    }
}

/// 各提供商默认 API 端点
pub fn default_base_url(provider: &str) -> &'static str {
    match provider {
        "openai" | "gpt"         => "https://api.openai.com/v1",
        "anthropic" | "claude"   => "https://api.anthropic.com/v1",
        "kimi"                   => "https://api.moonshot.cn/v1",
        "qwen"                   => "https://dashscope.aliyuncs.com/compatible-mode/v1",
        "minimax"                => "https://api.minimax.chat/v1",
        "deepseek"               => "https://api.deepseek.com/v1",
        "openrouter"             => "https://openrouter.ai/api/v1",
        _                        => "http://localhost:11434/v1",
    }
}

/// 使用消息历史调用聊天模型
pub async fn call_chat_model_with_messages(
    config: &ModelConfig,
    messages: &[OaiMessage],
) -> Result<String, String> {
    let request = OaiRequest {
        model: config.model.clone(),
        messages: messages.to_vec(),
        max_tokens: Some(2000),
        temperature: Some(0.7),
    };

    let url = format!("{}/chat/completions", config.base_url.trim_end_matches('/'));

    let client = reqwest::Client::new();
    let mut req = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .json(&request);

    if config.provider == "openrouter" {
        req = req
            .header("HTTP-Referer", "https://auto-heart.app")
            .header("X-Title", "Auto-Heart");
    }

    let response = req.send().await.map_err(|e| format!("HTTP error: {}", e))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("API error {}: {}", status, body));
    }

    let resp: OaiResponse = response.json().await
        .map_err(|e| format!("Parse error: {}", e))?;

    resp.choices.first()
        .map(|c| c.message.content.clone())
        .ok_or_else(|| "No response from model".into())
}
