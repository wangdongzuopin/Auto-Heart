use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Auto-Heart 持久化设置
///
/// 保存路径：{app_data_dir}/settings.json
/// 文档对齐：技术实现方案 §4 模型路由 + §5 沉默判断
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    // ── 基础配置 ──
    pub intent_doc_path: String,
    pub watch_paths: Vec<String>,
    pub silence_mode: String,
    pub offwork_time: String,
    pub dingtalk_webhook: String,
    pub feishu_webhook: String,

    // ── 每层心跳使用的模型 ──
    /// 中层心跳模型提供商：kimi | qwen | minimax | gpt | claude | deepseek | ollama | openrouter
    pub middle_model: String,
    /// 中层心跳具体模型名（空 = 使用提供商默认）
    pub middle_model_name: String,
    /// 深层心跳模型提供商
    pub deep_model: String,
    /// 深层心跳具体模型名
    pub deep_model_name: String,

    // ── API Keys ──
    pub kimi_api_key: String,
    pub claude_api_key: String,
    pub gpt_api_key: String,
    pub qwen_api_key: String,
    pub minimax_api_key: String,
    pub deepseek_api_key: String,
    /// OpenRouter API Key（统一代理，支持所有主流模型）
    pub openrouter_api_key: String,

    // ── Ollama 本地配置 ──
    /// Ollama 服务地址（默认 http://localhost:11434）
    pub ollama_base_url: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            intent_doc_path: String::new(),
            watch_paths: vec![],
            silence_mode: "normal".to_string(),
            offwork_time: "18:00".to_string(),
            dingtalk_webhook: String::new(),
            feishu_webhook: String::new(),
            middle_model: "kimi".to_string(),
            middle_model_name: String::new(),
            deep_model: "claude".to_string(),
            deep_model_name: String::new(),
            kimi_api_key: String::new(),
            claude_api_key: String::new(),
            gpt_api_key: String::new(),
            qwen_api_key: String::new(),
            minimax_api_key: String::new(),
            deepseek_api_key: String::new(),
            openrouter_api_key: String::new(),
            ollama_base_url: String::new(),
        }
    }
}

/// 线程安全的设置句柄
pub type SettingsHandle = Arc<Mutex<AppSettings>>;

pub fn load_settings(app_data_dir: &PathBuf) -> AppSettings {
    let path = app_data_dir.join("settings.json");
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(s) = serde_json::from_str::<AppSettings>(&content) {
                return s;
            }
        }
    }
    AppSettings::default()
}

pub fn save_settings_to_disk(app_data_dir: &PathBuf, settings: &AppSettings) -> Result<(), String> {
    let path = app_data_dir.join("settings.json");
    let content = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, content).map_err(|e| e.to_string())
}
