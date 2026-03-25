use crate::database::DbPool;
use crate::settings::{AppSettings, SettingsHandle};
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
