use crate::database::DbPool;
use crate::settings::{AppSettings, SettingsHandle, WindowState};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

// ──────────────────────────────────────────────
// 数据结构（前端消费）
// ──────────────────────────────────────────────

#[derive(Serialize, Clone)]
pub struct QueueMessage {
    pub id: String,
    pub priority: i32,
    pub title: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Serialize, Clone)]
pub struct SemanticModuleInfo {
    pub id: String,
    pub module_name: String,
    pub description: String,
    pub understanding: String,
    pub updated_at: String,
}

#[derive(Serialize, Clone)]
pub struct DecisionEntry {
    pub id: String,
    pub description: String,
    pub reason: String,
    pub related_file: String,
    pub created_at: String,
}

#[derive(Serialize, Clone)]
pub struct TechDebtEntry {
    pub id: String,
    pub description: String,
    pub impact: String,
    pub introduced_at: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TodayTask {
    pub time: String,
    pub task: String,
    pub tag: String,
    pub status: String, // "pending" | "active" | "done"
}

// ──────────────────────────────────────────────
// 消息队列命令
// ──────────────────────────────────────────────

/// 获取待处理的消息队列
#[tauri::command]
pub fn get_message_queue(db: State<'_, DbPool>) -> Vec<QueueMessage> {
    let db = match db.lock() {
        Ok(d) => d,
        Err(_) => return vec![],
    };
    let mut stmt = match db.prepare(
        "SELECT id, priority, title, content, created_at FROM message_queue \
         WHERE status = 'pending' ORDER BY priority ASC, created_at ASC LIMIT 20",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    stmt.query_map([], |row| {
        Ok(QueueMessage {
            id: row.get(0)?,
            priority: row.get(1)?,
            title: row.get(2)?,
            content: row.get(3)?,
            created_at: row.get(4)?,
        })
    })
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

/// 忽略/标记消息为已处理
#[tauri::command]
pub fn dismiss_message(id: String, db: State<'_, DbPool>) {
    if let Ok(db) = db.lock() {
        let _ = db.execute(
            "UPDATE message_queue SET status = 'dismissed', sent_at = datetime('now') WHERE id = ?1",
            rusqlite::params![id],
        );
    }
}

/// 标记消息为已发送（用户点了「帮我改」）
#[tauri::command]
pub fn ack_message(id: String, db: State<'_, DbPool>) {
    if let Ok(db) = db.lock() {
        let _ = db.execute(
            "UPDATE message_queue SET status = 'sent', sent_at = datetime('now') WHERE id = ?1",
            rusqlite::params![id],
        );
    }
}

// ──────────────────────────────────────────────
// 语义地图命令
// ──────────────────────────────────────────────

#[tauri::command]
pub fn get_semantic_modules(db: State<'_, DbPool>) -> Vec<SemanticModuleInfo> {
    let db = match db.lock() {
        Ok(d) => d,
        Err(_) => return vec![],
    };
    let mut stmt = match db.prepare(
        "SELECT id, module_name, description, understanding, updated_at \
         FROM semantic_modules ORDER BY updated_at DESC LIMIT 30",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    stmt.query_map([], |row| {
        Ok(SemanticModuleInfo {
            id: row.get(0)?,
            module_name: row.get(1)?,
            description: row.get(2)?,
            understanding: row.get(3)?,
            updated_at: row.get(4)?,
        })
    })
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

#[tauri::command]
pub fn get_decision_log(db: State<'_, DbPool>) -> Vec<DecisionEntry> {
    let db = match db.lock() {
        Ok(d) => d,
        Err(_) => return vec![],
    };
    let mut stmt = match db.prepare(
        "SELECT id, description, reason, related_file, created_at \
         FROM decision_log ORDER BY created_at DESC LIMIT 20",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    stmt.query_map([], |row| {
        Ok(DecisionEntry {
            id: row.get(0)?,
            description: row.get(1)?,
            reason: row.get(2)?,
            related_file: row.get(3)?,
            created_at: row.get(4)?,
        })
    })
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

#[tauri::command]
pub fn get_tech_debt(db: State<'_, DbPool>) -> Vec<TechDebtEntry> {
    let db = match db.lock() {
        Ok(d) => d,
        Err(_) => return vec![],
    };
    let mut stmt = match db.prepare(
        "SELECT id, description, impact, introduced_at FROM tech_debt \
         WHERE resolved_at IS NULL ORDER BY introduced_at DESC LIMIT 20",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    stmt.query_map([], |row| {
        Ok(TechDebtEntry {
            id: row.get(0)?,
            description: row.get(1)?,
            impact: row.get(2)?,
            introduced_at: row.get(3)?,
        })
    })
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

// ──────────────────────────────────────────────
// 今日任务命令
// ──────────────────────────────────────────────

/// 获取今日任务（从意图历史中读取今天的解析结果）
#[tauri::command]
pub fn get_today_tasks(db: State<'_, DbPool>) -> Vec<TodayTask> {
    let db = match db.lock() {
        Ok(d) => d,
        Err(_) => return vec![],
    };
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let mut stmt = match db.prepare(
        "SELECT parsed_tasks FROM intent_history WHERE date(created_at) = ?1 ORDER BY created_at DESC LIMIT 1",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let result: Option<String> = stmt
        .query_row(rusqlite::params![today], |row| row.get(0))
        .ok();

    if let Some(json_str) = result {
        if let Ok(tasks) = serde_json::from_str::<Vec<TodayTask>>(&json_str) {
            return tasks;
        }
    }
    vec![]
}

/// 获取今日意图原文（用于 TodayTab 展示）
#[tauri::command]
pub fn get_today_intent(db: State<'_, DbPool>) -> Option<serde_json::Value> {
    let db = match db.lock() {
        Ok(d) => d,
        Err(_) => return None,
    };
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    db.query_row(
        "SELECT raw_text, (parsed_tasks != '[]') as parsed \
         FROM intent_history WHERE date(created_at) = ?1 \
         ORDER BY created_at DESC LIMIT 1",
        rusqlite::params![today],
        |row| {
            Ok(serde_json::json!({
                "raw_text": row.get::<_, String>(0)?,
                "parsed": row.get::<_, bool>(1)?,
            }))
        },
    )
    .ok()
}

/// 手动添加意图（用户在设置中输入）
#[tauri::command]
pub fn add_intent(raw_text: String, db: State<'_, DbPool>) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    let id = Uuid::new_v4().to_string();
    db.execute(
        "INSERT INTO intent_history (id, raw_text, parsed_tasks) VALUES (?1, ?2, '[]')",
        rusqlite::params![id, raw_text],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

// ──────────────────────────────────────────────
// 设置命令
// ──────────────────────────────────────────────

#[tauri::command]
pub fn load_settings_cmd(settings: State<'_, SettingsHandle>) -> AppSettings {
    settings.lock().unwrap().clone()
}

#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    new_settings: AppSettings,
    settings: State<'_, SettingsHandle>,
) -> Result<(), String> {
    let app_data_dir: PathBuf = app.path().app_data_dir().map_err(|e| e.to_string())?;

    crate::settings::save_settings_to_disk(&app_data_dir, &new_settings)?;

    let mut s = settings.lock().unwrap();
    *s = new_settings;
    Ok(())
}

// ──────────────────────────────────────────────
// 日报命令
// ──────────────────────────────────────────────

#[derive(Serialize, Clone)]
pub struct ReportData {
    pub id: String,
    pub date: String,
    pub content: String,
    pub status: String, // draft | confirmed | sent
}

/// 获取今日日报（草稿或已发送）
#[tauri::command]
pub fn get_today_report(db: State<'_, DbPool>) -> Option<ReportData> {
    let db = match db.lock() {
        Ok(d) => d,
        Err(_) => return None,
    };
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    db.query_row(
        "SELECT id, date, content, status FROM daily_reports WHERE date = ?1 ORDER BY created_at DESC LIMIT 1",
        rusqlite::params![today],
        |row| {
            Ok(ReportData {
                id: row.get(0)?,
                date: row.get(1)?,
                content: row.get(2)?,
                status: row.get(3)?,
            })
        },
    )
    .ok()
}

/// 获取历史日报
#[tauri::command]
pub fn get_daily_report(date: String, db: State<'_, DbPool>) -> Option<ReportData> {
    let db = match db.lock() {
        Ok(d) => d,
        Err(_) => return None,
    };
    db.query_row(
        "SELECT id, date, content, status FROM daily_reports WHERE date = ?1 LIMIT 1",
        rusqlite::params![date],
        |row| {
            Ok(ReportData {
                id: row.get(0)?,
                date: row.get(1)?,
                content: row.get(2)?,
                status: row.get(3)?,
            })
        },
    )
    .ok()
}

/// 用户编辑日报内容
#[tauri::command]
pub fn update_report_content(
    date: String,
    content: String,
    db: State<'_, DbPool>,
) -> Result<(), String> {
    let db = db.lock().map_err(|e| e.to_string())?;
    db.execute(
        "UPDATE daily_reports SET content = ?1, status = 'confirmed' WHERE date = ?2",
        rusqlite::params![content, date],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 发送日报到指定渠道（dingtalk / feishu）
#[tauri::command]
pub async fn send_daily_report(
    date: String,
    channel: String,
    db: State<'_, DbPool>,
    settings: State<'_, SettingsHandle>,
) -> Result<(), String> {
    // 读取日报内容
    let content = {
        let db = db.lock().map_err(|e| e.to_string())?;
        db.query_row(
            "SELECT content FROM daily_reports WHERE date = ?1",
            rusqlite::params![date],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| format!("找不到 {} 的日报", date))?
    };

    let today_label = chrono::Local::now().format("%Y-%m-%d").to_string();
    let title = format!("Auto-Heart · 工作日报 {}", today_label);
    let full_text = format!("{}\n\n{}", title, content);

    let (dingtalk_url, feishu_url) = {
        let s = settings.lock().unwrap();
        (s.dingtalk_webhook.clone(), s.feishu_webhook.clone())
    };

    match channel.as_str() {
        "dingtalk" => crate::notifier::send_to_dingtalk(&dingtalk_url, &title, &content).await?,
        "feishu" => crate::notifier::send_to_feishu(&feishu_url, &full_text).await?,
        _ => return Err(format!("未知渠道: {}", channel)),
    }

    // 标记为已发送
    if let Ok(db) = db.lock() {
        let _ = db.execute(
            "UPDATE daily_reports SET status = 'sent' WHERE date = ?1",
            rusqlite::params![date],
        );
    }

    Ok(())
}

// ──────────────────────────────────────────────
// 操作日志 + 对话命令
// ──────────────────────────────────────────────

#[derive(Serialize)]
pub struct OperationLogEntry {
    pub id: String,
    pub timestamp: String,
    pub file_path: String,
    pub change_type: String,
    pub intention_desc: String,
    pub tags: Vec<String>,
}

#[derive(Serialize)]
pub struct TrendStats {
    pub days: i32,
    pub total_changes: i32,
    pub avg_per_day: f64,
    pub top_modules: Vec<String>,
}

/// 查询当日操作日志
#[tauri::command]
pub fn query_operation_logs(date: String, db: State<'_, DbPool>) -> Vec<OperationLogEntry> {
    let db = match db.lock() {
        Ok(d) => d,
        Err(_) => return vec![],
    };

    let date_filter = if date == "today" {
        "date(timestamp) = date('now')"
    } else {
        return vec![];
    };

    let mut stmt = match db.prepare(&format!(
        "SELECT id, timestamp, file_path, change_type, intention_desc, tags \
         FROM operation_log WHERE {} ORDER BY timestamp DESC LIMIT 100",
        date_filter
    )) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    stmt.query_map([], |row| {
        let tags_str: String = row.get(5)?;
        let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
        Ok(OperationLogEntry {
            id: row.get(0)?,
            timestamp: row.get(1)?,
            file_path: row.get(2)?,
            change_type: row.get(3)?,
            intention_desc: row.get(4)?,
            tags,
        })
    })
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

/// 搜索文件变更记录
#[tauri::command]
pub fn search_file_changes(
    keyword: String,
    days_back: Option<i64>,
    db: State<'_, DbPool>,
) -> Vec<(String, String, String)> {
    let db = match db.lock() {
        Ok(d) => d,
        Err(_) => return vec![],
    };

    let days = days_back.unwrap_or(7);
    let param = format!("-{} days", days);

    let mut stmt = match db.prepare(
        "SELECT file_path, change_type, timestamp FROM file_changes \
         WHERE file_path LIKE ?1 AND timestamp > datetime('now', ?2) \
         ORDER BY timestamp DESC LIMIT 50",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let pattern = format!("%{}%", keyword);
    stmt.query_map(rusqlite::params![pattern, param], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

/// 获取趋势统计
#[tauri::command]
pub fn get_trend_stats(days: i64, db: State<'_, DbPool>) -> TrendStats {
    let db = match db.lock() {
        Ok(d) => d,
        Err(_) => return TrendStats { days: days as i32, total_changes: 0, avg_per_day: 0.0, top_modules: vec![] },
    };

    let param = format!("-{} days", days);
    let total: i32 = db.query_row(
        "SELECT COUNT(*) FROM file_changes WHERE timestamp > datetime('now', ?1)",
        rusqlite::params![param],
        |_row| _row.get(0),
    ).unwrap_or(0);

    let avg = total as f64 / days as f64;

    let mut stmt = match db.prepare(
        "SELECT file_path, COUNT(*) as cnt FROM file_changes \
         WHERE timestamp > datetime('now', ?1) \
         GROUP BY file_path ORDER BY cnt DESC LIMIT 5",
    ) {
        Ok(s) => s,
        Err(_) => return TrendStats { days: days as i32, total_changes: total, avg_per_day: avg, top_modules: vec![] },
    };

    let top: Vec<String> = stmt.query_map(rusqlite::params![param], |row| {
        row.get::<_, String>(0)
    })
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default();

    TrendStats {
        days: days as i32,
        total_changes: total,
        avg_per_day: avg,
        top_modules: top,
    }
}

// ──────────────────────────────────────────────
// 窗口状态命令
// ──────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
pub struct MainWindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_maximized: bool,
}

impl From<MainWindowState> for WindowState {
    fn from(s: MainWindowState) -> Self {
        WindowState {
            x: s.x,
            y: s.y,
            width: s.width,
            height: s.height,
            is_maximized: s.is_maximized,
        }
    }
}

impl From<WindowState> for MainWindowState {
    fn from(s: WindowState) -> Self {
        MainWindowState {
            x: s.x,
            y: s.y,
            width: s.width,
            height: s.height,
            is_maximized: s.is_maximized,
        }
    }
}

#[tauri::command]
pub fn save_window_state(state: MainWindowState, settings: State<'_, SettingsHandle>) -> Result<(), String> {
    let mut s = settings.lock().unwrap();
    s.last_window_state = Some(state.into());
    Ok(())
}

#[tauri::command]
pub fn load_window_state(settings: State<'_, SettingsHandle>) -> Option<MainWindowState> {
    let s = settings.lock().unwrap();
    s.last_window_state.clone().map(|ws| ws.into())
}

// ──────────────────────────────────────────────
// 对话命令
// ──────────────────────────────────────────────

#[derive(Serialize)]
pub struct ConversationInfo {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
}

#[derive(Serialize)]
pub struct ChatMessage {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: String,
}

#[tauri::command]
pub fn get_conversations(app: AppHandle) -> Vec<ConversationInfo> {
    let data_dir = app.path().app_data_dir().unwrap_or_default();
    let conversations = crate::conversation::list_conversations(&data_dir);

    conversations
        .into_iter()
        .map(|c| ConversationInfo {
            id: c.id,
            title: c.title,
            created_at: c.created_at,
            updated_at: c.updated_at,
            message_count: c.messages.len(),
        })
        .collect()
}

#[tauri::command]
pub fn get_conversation(id: String, app: AppHandle) -> Option<crate::conversation::Conversation> {
    let data_dir = app.path().app_data_dir().unwrap_or_default();
    crate::conversation::get_conversation(&data_dir, &id)
}

#[tauri::command]
pub fn create_conversation(first_message: String, app: AppHandle) -> Result<ConversationInfo, String> {
    let data_dir = app.path().app_data_dir().unwrap_or_default();
    let conv = crate::conversation::Conversation::new(&first_message);
    crate::conversation::save_conversation(&data_dir, &conv)?;

    Ok(ConversationInfo {
        id: conv.id,
        title: conv.title,
        created_at: conv.created_at,
        updated_at: conv.updated_at,
        message_count: 0,
    })
}

#[tauri::command]
pub fn delete_conversation(id: String, app: AppHandle) -> Result<(), String> {
    let data_dir = app.path().app_data_dir().unwrap_or_default();
    crate::conversation::delete_conversation(&data_dir, &id)
}

#[tauri::command]
pub async fn send_message(
    session_id: String,
    content: String,
    app: AppHandle,
    settings: State<'_, SettingsHandle>,
    db: State<'_, DbPool>,
) -> Result<ChatMessage, String> {
    let data_dir = app.path().app_data_dir().unwrap_or_default();

    // 获取或创建会话
    let mut conv = crate::conversation::get_conversation(&data_dir, &session_id)
        .unwrap_or_else(|| crate::conversation::Conversation::new(&content));

    // 添加用户消息
    let user_msg = crate::conversation::Message {
        id: Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: content.clone(),
        timestamp: chrono::Local::now().to_rfc3339(),
    };
    conv.messages.push(user_msg.clone());
    conv.updated_at = chrono::Local::now().to_rfc3339();
    crate::conversation::save_conversation(&data_dir, &conv)?;

    // 调用 LLM
    let settings_snap = settings.lock().unwrap().clone();

    // 构建消息历史
    let oai_messages: Vec<crate::model_router::OaiMessage> = conv.messages.iter().map(|m| {
        crate::model_router::OaiMessage {
            role: m.role.clone(),
            content: m.content.clone(),
        }
    }).collect();

    // 调用模型
    let model_config = crate::model_router::build_model_config(
        &settings_snap.chat_model,
        &settings_snap.chat_model_name,
        &settings_snap,
    ).ok_or("Chat model not configured. Please set chat_model in settings.")?;

    let response = crate::model_router::call_chat_model_with_messages(
        &model_config,
        &oai_messages,
    ).await?;

    // 添加助手消息
    let assistant_msg = crate::conversation::Message {
        id: Uuid::new_v4().to_string(),
        role: "assistant".to_string(),
        content: response.clone(),
        timestamp: chrono::Local::now().to_rfc3339(),
    };

    // 更新会话
    conv.messages.push(assistant_msg.clone());
    conv.updated_at = chrono::Local::now().to_rfc3339();
    crate::conversation::save_conversation(&data_dir, &conv)?;

    // 检查是否包含意图关键词
    if contains_intent_keywords(&content) {
        let _ = parse_intent_from_chat(&content, &settings_snap, &db);
    }

    Ok(ChatMessage {
        id: assistant_msg.id,
        role: assistant_msg.role,
        content: assistant_msg.content,
        timestamp: assistant_msg.timestamp,
    })
}

/// 检查消息是否包含意图关键词
fn contains_intent_keywords(content: &str) -> bool {
    let keywords = ["今天", "要做", "待办", "计划", "任务", "完成", "开始", "帮我"];
    keywords.iter().any(|k| content.contains(k))
}

/// 从聊天内容解析意图并写入数据库
fn parse_intent_from_chat(content: &str, settings: &AppSettings, db: &DbPool) -> Option<()> {
    let prompt = format!(
        "用户消息：{}\n\n请解析为任务列表，输出 JSON 数组（无其他内容）：\n[{{\"time\":\"HH:MM\",\"task\":\"任务描述\",\"tag\":\"关联模块\",\"status\":\"pending\"}}]",
        content
    );

    let config = crate::model_router::build_model_config(
        &settings.middle_model,
        &settings.middle_model_name,
        settings,
    )?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build().ok()?;

    let router = crate::model_router::ModelRouter::new();
    let response = rt.block_on(router.call_with_config(
        &config,
        &prompt,
        Some("你是 Auto-Heart，解析用户任务。只输出 JSON 数组。"),
    )).ok()?;

    // 提取 JSON
    let json_str = extract_json_array(&response);
    if let Ok(tasks) = serde_json::from_str::<serde_json::Value>(json_str) {
        if let Some(tasks_array) = tasks.as_array() {
            if !tasks_array.is_empty() {
                let tasks_json = serde_json::to_string(&tasks_array).unwrap_or_default();
                if let Ok(db) = db.lock() {
                    let id = Uuid::new_v4().to_string();
                    let _ = db.execute(
                        "INSERT INTO intent_history (id, raw_text, parsed_tasks, completion_status) VALUES (?1, ?2, ?3, 'active')",
                        rusqlite::params![id, content, tasks_json],
                    );
                }
            }
        }
    }

    Some(())
}

/// 从文本中提取 JSON 数组
fn extract_json_array(text: &str) -> &str {
    if let Some(start) = text.find("```json") {
        let inner = &text[start + 7..];
        if let Some(end) = inner.find("```") {
            return inner[..end].trim();
        }
    }
    if let (Some(s), Some(e)) = (text.find('['), text.rfind(']')) {
        return &text[s..=e];
    }
    text
}
