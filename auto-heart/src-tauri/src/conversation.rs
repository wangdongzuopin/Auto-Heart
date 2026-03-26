use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

/// 对话消息结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: String, // "user" | "assistant"
    pub content: String,
    pub timestamp: String,
}

/// 对话会话结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,       // 对话标题（取首条用户消息前20字）
    pub created_at: String,
    pub updated_at: String,
    pub messages: Vec<Message>,
}

impl Conversation {
    pub fn new(first_message: &str) -> Self {
        let now = chrono::Local::now().to_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            title: first_message.chars().take(20).collect(),
            created_at: now.clone(),
            updated_at: now,
            messages: vec![],
        }
    }
}

/// 获取对话存储目录
pub fn get_conversations_dir(data_dir: &PathBuf) -> PathBuf {
    let dir = data_dir.join("conversations");
    std::fs::create_dir_all(&dir).ok();
    dir
}

/// 列出所有对话
pub fn list_conversations(data_dir: &PathBuf) -> Vec<Conversation> {
    let dir = get_conversations_dir(data_dir);
    let mut conversations = vec![];

    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if entry.path().extension().map_or(false, |e| e == "json") {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    if let Ok(conv) = serde_json::from_str::<Conversation>(&content) {
                        conversations.push(conv);
                    }
                }
            }
        }
    }

    // 按更新时间倒序
    conversations.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    conversations
}

/// 保存对话到文件
pub fn save_conversation(data_dir: &PathBuf, conv: &Conversation) -> Result<(), String> {
    let path = get_conversations_dir(data_dir).join(format!("{}.json", conv.id));
    let content = serde_json::to_string_pretty(conv).map_err(|e| e.to_string())?;
    std::fs::write(path, content).map_err(|e| e.to_string())
}

/// 获取单个对话
pub fn get_conversation(data_dir: &PathBuf, id: &str) -> Option<Conversation> {
    let path = get_conversations_dir(data_dir).join(format!("{}.json", id));
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// 删除对话
pub fn delete_conversation(data_dir: &PathBuf, id: &str) -> Result<(), String> {
    let path = get_conversations_dir(data_dir).join(format!("{}.json", id));
    std::fs::remove_file(path).map_err(|e| e.to_string())
}