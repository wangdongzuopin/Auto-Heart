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
    /// 自定义数据根目录（空 = 使用默认 app_data_dir）
    /// 实际数据路径：{data_dir}/YYYY-MM-DD/
    pub data_dir: String,
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
    pub chat_model: String,
    pub chat_model_name: String,

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

    // ── 主动建议 ──
    pub proactive_suggestions: bool,
    pub critical_keywords: String,

    // ── 窗口状态 ──
    pub last_window_state: Option<WindowState>,
}

/// 窗口状态结构体（用于窗口位置/大小记忆）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_maximized: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            data_dir: String::new(),
            watch_paths: vec![],
            silence_mode: "normal".to_string(),
            offwork_time: "18:00".to_string(),
            dingtalk_webhook: String::new(),
            feishu_webhook: String::new(),
            middle_model: "kimi".to_string(),
            middle_model_name: String::new(),
            deep_model: "claude".to_string(),
            deep_model_name: String::new(),
            chat_model: "kimi".to_string(),
            chat_model_name: String::new(),
            kimi_api_key: String::new(),
            claude_api_key: String::new(),
            gpt_api_key: String::new(),
            qwen_api_key: String::new(),
            minimax_api_key: String::new(),
            deepseek_api_key: String::new(),
            openrouter_api_key: String::new(),
            ollama_base_url: String::new(),
            proactive_suggestions: true,
            critical_keywords: "auth,security,password,token,payment,config,middleware,permission".to_string(),
            last_window_state: None,
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

/// 自动检测常见开发目录（当 watch_paths 为空时调用）
pub fn auto_detect_watch_paths() -> Vec<String> {
    let mut paths = vec![];

    #[cfg(target_os = "windows")]
    {
        if let Ok(user_dir) = std::env::var("USERPROFILE") {
            let base = PathBuf::from(&user_dir);
            // 常见开发目录
            for name in &["Documents", "Desktop", "Code", "Projects"] {
                let path = base.join(name);
                if path.exists() {
                    paths.push(path.to_string_lossy().to_string());
                }
            }
            // 也检查用户主目录本身（有些人在 ~/dev 下开发）
            let home_dev = base.join("dev");
            if home_dev.exists() {
                paths.push(home_dev.to_string_lossy().to_string());
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            let base = PathBuf::from(&home);
            for name in &["Documents", "Code", "Projects", "Developer"] {
                let path = base.join(name);
                if path.exists() {
                    paths.push(path.to_string_lossy().to_string());
                }
            }
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if let Ok(home) = std::env::var("HOME") {
            let base = PathBuf::from(&home);
            for name in &["Documents", "Code", "projects", "dev"] {
                let path = base.join(name);
                if path.exists() {
                    paths.push(path.to_string_lossy().to_string());
                }
            }
        }
    }

    paths
}
