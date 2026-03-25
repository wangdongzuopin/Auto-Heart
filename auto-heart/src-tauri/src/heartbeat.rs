use crate::database::DbPool;
use crate::model_router::ModelRouter;
use crate::settings::SettingsHandle;
use chrono::Timelike;
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

// ──────────────────────────────────────────────
// 浅层心跳 — 每 30 秒（本地规则，0 token）
// ──────────────────────────────────────────────

/// 技术文档 §2.2：感知文件变更、活跃应用、意图文档、行为信号
pub fn start_shallow_heartbeat(app: AppHandle, db: DbPool, settings: SettingsHandle) {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(30));

            // 1. 活跃应用检测
            let active_app = get_active_app();
            if !active_app.is_empty() && active_app != "unknown" {
                log_active_app(&db, &active_app);
            }

            // 2. 检查意图文档是否更新
            let intent_path = { settings.lock().unwrap().intent_doc_path.clone() };
            if !intent_path.is_empty() {
                check_intent_doc_update(&db, &intent_path);
            }

            // 3. 自然节点检测：若 90 秒内无文件变更，释放 P1 消息
            let has_pending = check_and_flush_pending_messages(&app, &db);
            if has_pending {
                let _ = app.emit("message_queue:flush", ());
            }

            let _ = app.emit("heartbeat:shallow", ());
        }
    });
}

/// 获取当前前台活跃应用名称（跨平台）
fn get_active_app() -> String {
    #[cfg(target_os = "windows")]
    {
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "try { (Get-Process | Where-Object {$_.MainWindowHandle -ne 0 -and $_.MainWindowTitle -ne ''} | Sort-Object CPU -Descending | Select-Object -First 1).ProcessName } catch { '' }",
            ])
            .output();
        match output {
            Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            Err(_) => "unknown".to_string(),
        }
    }
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("osascript")
            .args([
                "-e",
                "tell application \"System Events\" to get name of first application process whose frontmost is true",
            ])
            .output();
        match output {
            Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            Err(_) => "unknown".to_string(),
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        "unknown".to_string()
    }
}

/// 记录活跃应用到 DB（用于行为分析）
fn log_active_app(db: &DbPool, app_name: &str) {
    if let Ok(db) = db.lock() {
        // 使用 kv 表记录，复用 decision_log context 字段存储活跃 app 日志
        let _ = db.execute(
            "INSERT OR IGNORE INTO decision_log (id, description, reason, related_file, context) \
             VALUES (?1, 'active_app_log', ?2, '', datetime('now'))",
            rusqlite::params![Uuid::new_v4().to_string(), format!("active:{}", app_name)],
        );
    }
}

/// 检查意图文档是否更新，读取真实内容写入 DB 等待中层解析
///
/// 技术文档 §3.3：检测新内容 → 写入 intent_history → 等待 LLM 解析
fn check_intent_doc_update(db: &DbPool, path: &str) {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return,
    };
    let modified = match meta.modified() {
        Ok(t) => t,
        Err(_) => return,
    };
    // 只处理 35 秒内的新变更
    let elapsed = modified.elapsed().unwrap_or(Duration::from_secs(999));
    if elapsed.as_secs() >= 35 {
        return;
    }

    // 读取文件实际内容
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c.trim().to_string(),
        Err(_) => return,
    };
    if content.is_empty() {
        return;
    }

    let db = match db.lock() {
        Ok(d) => d,
        Err(_) => return,
    };

    // 今天是否已有相同内容（避免重复写入）
    let already_exists: bool = db
        .query_row(
            "SELECT COUNT(*) FROM intent_history WHERE raw_text = ?1 AND date(created_at) = date('now')",
            rusqlite::params![content],
            |row| row.get::<_, i32>(0),
        )
        .map(|c| c > 0)
        .unwrap_or(false);

    if already_exists {
        return;
    }

    // 写入真实内容，parsed_tasks = '[]' 表示等待中层心跳解析
    let _ = db.execute(
        "INSERT INTO intent_history (id, raw_text, parsed_tasks, completion_status) VALUES (?1, ?2, '[]', 'pending')",
        rusqlite::params![Uuid::new_v4().to_string(), content],
    );
    eprintln!("[shallow] 检测到意图文档更新，已写入 DB 等待解析");
}

/// 用户活跃状态（技术文档 §5.1 动态状态推断）
#[derive(Debug, PartialEq)]
enum ActivityLevel {
    Flow,   // 5 分钟内 ≥5 次变更 → 保护 flow，不打扰
    Normal, // 有活动但不密集
    Idle,   // 90s 内无任何变更 → 自然节点，可以发消息
}

/// 根据 file_changes 频率计算当前活跃度（动态状态推断）
fn get_activity_level(db: &DbPool) -> ActivityLevel {
    let db = match db.lock() {
        Ok(d) => d,
        Err(_) => return ActivityLevel::Normal,
    };

    // 90s 内有变更 → 非 Idle
    let count_90s: i32 = db
        .query_row(
            "SELECT COUNT(*) FROM file_changes WHERE timestamp > datetime('now', '-90 seconds')",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if count_90s == 0 {
        return ActivityLevel::Idle;
    }

    // 5 分钟内 ≥5 次 → Flow 状态，不打扰
    let count_5min: i32 = db
        .query_row(
            "SELECT COUNT(*) FROM file_changes WHERE timestamp > datetime('now', '-5 minutes')",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    if count_5min >= 5 {
        ActivityLevel::Flow
    } else {
        ActivityLevel::Normal
    }
}

/// 检查是否有 P1 消息等待自然节点释放
///
/// 技术文档 §5.3：自然节点 = 打字停顿 90s → 动态阈值降低 → 释放 P1
fn check_and_flush_pending_messages(app: &AppHandle, db: &DbPool) -> bool {
    // Flow 状态绝不打扰，Normal 状态也等 Idle
    let level = get_activity_level(db);
    if level != ActivityLevel::Idle {
        return false;
    }

    let db = match db.lock() {
        Ok(d) => d,
        Err(_) => return false,
    };

    // 释放 P1 pending 消息（发给前端）
    let mut stmt = match db.prepare(
        "SELECT id, title, content FROM message_queue \
         WHERE status = 'pending' AND priority = 1 \
         AND created_at < datetime('now', '-90 seconds') LIMIT 3",
    ) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let messages: Vec<(String, String, String)> = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default();

    if messages.is_empty() {
        return false;
    }

    // 只释放第一条（避免轰炸用户）
    if let Some((id, title, content)) = messages.into_iter().next() {
        let _ = app.emit(
            "message_queue:new",
            serde_json::json!({
                "id": id,
                "title": title,
                "content": content,
                "priority": 1,
            }),
        );
        return true;
    }
    false
}

// ──────────────────────────────────────────────
// 中层心跳 — 每 10 分钟（轻量模型，低 token）
// ──────────────────────────────────────────────

/// 技术文档 §2.3：收集 diff → 轻量模型 → 更新语义地图 → 消息队列
/// 技术文档 §3.3：解析待处理的意图文档 → 结构化任务 + 时间点 + 代码关联
pub fn start_middle_heartbeat(app: AppHandle, db: DbPool, settings: SettingsHandle) {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(600));

            let _ = app.emit("heartbeat:middle", ());

            // 快照设置（避免长时间持有锁）
            let settings_snap = { settings.lock().unwrap().clone() };

            // 检查中层模型是否已配置
            if crate::model_router::build_model_config(
                &settings_snap.middle_model,
                &settings_snap.middle_model_name,
                &settings_snap,
            ).is_none() {
                eprintln!("[middle] 中层模型「{}」未配置 API Key，跳过", settings_snap.middle_model);
                continue;
            }

            let silence_mode = settings_snap.silence_mode.clone();

            // 任务 A：解析待处理的意图文档（技术文档 §3.3）
            parse_pending_intent_docs(&db, &app, &settings_snap);

            // 任务 B：文件变更 → 语义地图更新（技术文档 §2.3）
            let changes = collect_file_changes(&db, 11);
            if !changes.is_empty() {
                if let Some(response) = call_semantic_model(&settings_snap, &changes) {
                    process_semantic_response(&db, &app, &response, &silence_mode);
                }
            }
        }
    });
}

/// 收集最近 N 分钟文件变更列表
fn collect_file_changes(db: &DbPool, minutes_back: i64) -> Vec<String> {
    let db = match db.lock() {
        Ok(d) => d,
        Err(_) => return vec![],
    };
    let mut stmt = match db.prepare(
        "SELECT DISTINCT file_path, change_type FROM file_changes \
         WHERE timestamp > datetime('now', ?1) \
         AND file_path NOT LIKE '%.git%' \
         LIMIT 30",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let param = format!("-{} minutes", minutes_back);
    stmt.query_map(rusqlite::params![param], |row| {
        Ok(format!(
            "{} [{}]",
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?
        ))
    })
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

/// 调用中层模型生成语义理解（模型由 settings.middle_model 决定）
fn call_semantic_model(settings: &crate::settings::AppSettings, changes: &[String]) -> Option<String> {
    let changes_text = changes.join("\n");
    let prompt = format!(
        "以下是开发者过去10分钟的文件变更：\n{}\n\n请分析并只输出以下 JSON（无其他内容）：\n{{\"modules\":[{{\"name\":\"模块名\",\"description\":\"职责描述\",\"understanding\":\"本次变更的业务含义\"}}],\"issues\":[{{\"priority\":\"P1\",\"title\":\"问题标题(<=20字)\",\"content\":\"问题描述(<=60字)\"}}]}}",
        changes_text
    );

    let config = crate::model_router::build_model_config(
        &settings.middle_model,
        &settings.middle_model_name,
        settings,
    )?;

    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().ok()?;
    let router = ModelRouter::new();
    rt.block_on(router.call_with_config(
        &config,
        &prompt,
        Some("你是 Auto-Heart，只输出 JSON，不要 markdown 代码块。"),
    )).ok()
}

/// LLM 响应结构
#[derive(Deserialize)]
struct SemanticResponse {
    modules: Vec<ModuleUpdate>,
    #[serde(default)]
    issues: Vec<IssueReport>,
}

#[derive(Deserialize)]
struct ModuleUpdate {
    name: String,
    description: String,
    understanding: String,
}

#[derive(Deserialize)]
struct IssueReport {
    priority: String,
    title: String,
    content: String,
}

/// 从模型响应中提取 JSON 部分（兼容 ```json ... ``` 包装）
fn extract_json(text: &str) -> &str {
    if let Some(start) = text.find("```json") {
        let inner = &text[start + 7..];
        if let Some(end) = inner.find("```") {
            return inner[..end].trim();
        }
    }
    // 找最外层 { ... }
    if let (Some(s), Some(e)) = (text.find('{'), text.rfind('}')) {
        return &text[s..=e];
    }
    text
}

/// 将模型响应写入语义地图 + 消息队列，并向前端发事件
fn process_semantic_response(db: &DbPool, app: &AppHandle, response: &str, silence_mode: &str) {
    let json_str = extract_json(response);
    let parsed: SemanticResponse = match serde_json::from_str(json_str) {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "[middle] parse error: {} | raw: {}",
                e,
                &response[..response.len().min(200)]
            );
            return;
        }
    };

    // 写语义地图（收集 emit 列表后再发）
    let mut to_emit: Vec<(String, String, String, i32)> = vec![];

    {
        let db = match db.lock() {
            Ok(d) => d,
            Err(_) => return,
        };

        // 更新模块记录
        for m in &parsed.modules {
            let id = Uuid::new_v4().to_string();
            let _ = db.execute(
                "INSERT INTO semantic_modules (id, module_name, description, understanding, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, datetime('now')) \
                 ON CONFLICT(module_name) DO UPDATE SET \
                   description = excluded.description, \
                   understanding = excluded.understanding, \
                   updated_at = excluded.updated_at",
                rusqlite::params![id, m.name, m.description, m.understanding],
            );
        }

        // 写消息队列
        for issue in &parsed.issues {
            let priority: i32 = match issue.priority.as_str() {
                "P0" => 0,
                "P1" => 1,
                _ => 2,
            };

            // 根据沉默阈值过滤
            let skip = match silence_mode {
                "focus" => priority > 0,
                "normal" => priority > 1,
                _ => false, // "open" 全部接收
            };

            if skip {
                continue;
            }

            let id = Uuid::new_v4().to_string();
            let _ = db.execute(
                "INSERT INTO message_queue (id, priority, title, content, status) VALUES (?1, ?2, ?3, ?4, 'pending')",
                rusqlite::params![id, priority, issue.title, issue.content],
            );

            // P0 立即通知，P1 等自然节点（浅层心跳会释放）
            if priority == 0 {
                to_emit.push((id, issue.title.clone(), issue.content.clone(), priority));
            }
        }
    } // 释放 DB 锁

    for (id, title, content, priority) in to_emit {
        let _ = app.emit(
            "message_queue:new",
            serde_json::json!({
                "id": id,
                "title": title,
                "content": content,
                "priority": priority,
            }),
        );
    }
}

// ──────────────────────────────────────────────
// 深层心跳 — 每日（强模型，日报生成）
// ──────────────────────────────────────────────

/// 技术文档 §2.4：读取全天语义更新 → 调用 Claude → 生成日报
pub fn start_deep_heartbeat(app: AppHandle, db: DbPool, settings: SettingsHandle) {
    thread::spawn(move || {
        loop {
            // 每 30 分钟检查一次是否到下班时间
            thread::sleep(Duration::from_secs(1800));

            let settings_snap = { settings.lock().unwrap().clone() };

            // 检查深层模型是否已配置
            if crate::model_router::build_model_config(
                &settings_snap.deep_model,
                &settings_snap.deep_model_name,
                &settings_snap,
            ).is_none() {
                continue;
            }

            // 下班检测：时间信号 OR 行为信号（文档§2.5：三信号任意两个满足）
            let time_signal = is_offwork_time(&settings_snap.offwork_time);
            let behavior_signal = is_inactive_for_minutes(&db, 30);
            if !time_signal && !behavior_signal {
                continue;
            }
            eprintln!(
                "[deep] 下班检测触发 time={} behavior={}",
                time_signal, behavior_signal
            );

            // 检查今天是否已生成日报
            let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
            let already_done: bool = {
                let db = match db.lock() {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                db.query_row(
                    "SELECT COUNT(*) FROM daily_reports WHERE date = ?1",
                    rusqlite::params![today],
                    |row| row.get::<_, i32>(0),
                )
                .map(|c| c > 0)
                .unwrap_or(false)
            };

            if already_done {
                continue;
            }

            let _ = app.emit("heartbeat:deep", ());
            eprintln!("[deep] 触发深层心跳，开始生成日报...");

            if let Some(report) = generate_daily_report(&db, &settings_snap, &today) {
                save_daily_report(&db, &today, &report);
                let _ = app.emit(
                    "daily_report:ready",
                    serde_json::json!({ "date": today, "preview": &report[..report.len().min(100)] }),
                );
                eprintln!("[deep] 日报生成完成");
            }
        }
    });
}

/// 判断是否到下班时间（±15 分钟容差）
fn is_offwork_time(offwork_time: &str) -> bool {
    let parts: Vec<&str> = offwork_time.split(':').collect();
    if parts.len() != 2 {
        return false;
    }
    let (h, m): (u32, u32) = match (parts[0].parse(), parts[1].parse()) {
        (Ok(h), Ok(m)) => (h, m),
        _ => return false,
    };

    let now = chrono::Local::now();
    let offwork_mins = h * 60 + m;
    let current_mins = now.hour() * 60 + now.minute();

    // 在下班时间 ±15 分钟内触发
    (current_mins as i32 - offwork_mins as i32).abs() <= 15
}

/// 收集今日数据并调用深层模型生成日报（模型由 settings.deep_model 决定）
fn generate_daily_report(db: &DbPool, settings: &crate::settings::AppSettings, date: &str) -> Option<String> {
    // 收集今日文件变更摘要
    let file_changes = {
        let db = match db.lock() {
            Ok(d) => d,
            Err(_) => return None,
        };
        let mut stmt = match db.prepare(
            "SELECT DISTINCT file_path FROM file_changes WHERE date(timestamp) = ?1 LIMIT 20",
        ) {
            Ok(s) => s,
            Err(_) => return None,
        };
        stmt.query_map(rusqlite::params![date], |row| row.get::<_, String>(0))
            .map(|rows| rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
            .unwrap_or_default()
    };

    // 收集今日语义地图更新
    let semantic_updates = {
        let db = match db.lock() {
            Ok(d) => d,
            Err(_) => return None,
        };
        let mut stmt = match db.prepare(
            "SELECT module_name, understanding FROM semantic_modules WHERE date(updated_at) = ?1",
        ) {
            Ok(s) => s,
            Err(_) => return None,
        };
        stmt.query_map(rusqlite::params![date], |row| {
            Ok(format!(
                "{}: {}",
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?
            ))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
        .unwrap_or_default()
    };

    if file_changes.is_empty() && semantic_updates.is_empty() {
        return None;
    }

    let prompt = format!(
        "今日是 {}。\n\n涉及文件：\n{}\n\n语义理解：\n{}\n\n请生成一份有业务意义的工作日报（200字以内）。\n要求：\n- 说明完成了什么功能/修复了什么问题\n- 体现业务价值，而非简单罗列文件\n- 指出明日可能需要关注的事项",
        date,
        file_changes.join("\n"),
        if semantic_updates.is_empty() { "暂无语义更新".to_string() } else { semantic_updates.join("\n") }
    );

    let config = crate::model_router::build_model_config(
        &settings.deep_model,
        &settings.deep_model_name,
        settings,
    )?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()?;

    let router = ModelRouter::new();

    rt.block_on(router.call_with_config(
        &config,
        &prompt,
        Some("你是 Auto-Heart，一个理解开发者项目的 AI 助理。请生成简洁、有业务价值的日报。"),
    ))
    .ok()
}

/// 保存日报到 DB
fn save_daily_report(db: &DbPool, date: &str, content: &str) {
    if let Ok(db) = db.lock() {
        let id = Uuid::new_v4().to_string();
        let _ = db.execute(
            "INSERT OR REPLACE INTO daily_reports (id, date, content, status) VALUES (?1, ?2, ?3, 'draft')",
            rusqlite::params![id, date, content],
        );
    }
}

// ──────────────────────────────────────────────
// 文件系统监听器
// ──────────────────────────────────────────────

// ──────────────────────────────────────────────
// 意图文档解析（技术文档 §3.3）
// ──────────────────────────────────────────────

/// 解析今日所有 parsed_tasks = '[]' 的意图记录
///
/// 流程：读取 raw_text → 调用 Kimi → 提取 JSON 任务列表 → 关联代码模块 → 写回 DB → 发事件
fn parse_pending_intent_docs(db: &DbPool, app: &AppHandle, settings: &crate::settings::AppSettings) {
    // 查找今天未解析的记录
    let pending: Vec<(String, String)> = {
        let db = match db.lock() {
            Ok(d) => d,
            Err(_) => return,
        };
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let mut stmt = match db.prepare(
            "SELECT id, raw_text FROM intent_history \
             WHERE parsed_tasks = '[]' AND date(created_at) = ?1 LIMIT 3",
        ) {
            Ok(s) => s,
            Err(_) => return,
        };
        stmt.query_map(rusqlite::params![today], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    };

    if pending.is_empty() {
        return;
    }

    // 获取已有语义地图模块（用于代码上下文关联）
    let modules_context = get_modules_context(db);

    for (id, raw_text) in pending {
        eprintln!("[middle] 解析意图文档 id={}", &id[..8]);
        if let Some(parsed_json) = call_intent_parser(settings, &raw_text, &modules_context) {
            // 写回 parsed_tasks
            if let Ok(db) = db.lock() {
                let _ = db.execute(
                    "UPDATE intent_history SET parsed_tasks = ?1, completion_status = 'active' WHERE id = ?2",
                    rusqlite::params![parsed_json, id],
                );
            }
            // 通知前端刷新任务列表
            let _ = app.emit("intent:parsed", ());
            eprintln!("[middle] 意图解析完成，已通知前端");
        }
    }
}

/// 获取语义地图模块摘要（用于意图文档解析时的代码关联）
fn get_modules_context(db: &DbPool) -> String {
    let db = match db.lock() {
        Ok(d) => d,
        Err(_) => return String::new(),
    };
    let mut stmt = match db.prepare(
        "SELECT module_name, description FROM semantic_modules ORDER BY updated_at DESC LIMIT 15",
    ) {
        Ok(s) => s,
        Err(_) => return String::new(),
    };
    stmt.query_map([], |row| {
        Ok(format!(
            "- {}: {}",
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?
        ))
    })
    .map(|rows| rows.filter_map(|r| r.ok()).collect::<Vec<_>>().join("\n"))
    .unwrap_or_default()
}

/// 调用 Kimi 将自然语言意图文档解析为结构化任务列表
///
/// 技术文档 §3.3：解析时间点 + 自动关联代码上下文
fn call_intent_parser(settings: &crate::settings::AppSettings, raw_text: &str, modules_context: &str) -> Option<String> {
    let now = chrono::Local::now();
    let today_str = now.format("%Y-%m-%d %H:%M").to_string();

    let modules_section = if modules_context.is_empty() {
        String::new()
    } else {
        format!("\n已知项目模块（用于关联代码）：\n{}\n", modules_context)
    };

    let prompt = format!(
        "今天是 {today}。\n\
         用户工作意图文档：\n{content}\n\
         {modules}\n\
         请解析所有待办任务，只输出 JSON 数组（无其他内容，无 markdown）：\n\
         [{{\"time\":\"HH:MM\",\"task\":\"任务描述\",\"tag\":\"关联模块或文件\",\"status\":\"pending\"}}]\n\n\
         时间解析规则（24小时制）：\n\
         - \"上午\" / \"早上\" → \"09:00\"\n\
         - \"X点\" → \"X:00\"（如 \"10点\" → \"10:00\"）\n\
         - \"X:XX\" → 直接使用\n\
         - \"下午\" / \"午后\" → \"14:00\"\n\
         - \"下班前\" / \"晚些\" → \"17:30\"\n\
         - 没有明确时间 → \"\"（空字符串）\n\
         代码关联：根据任务描述和已知模块，推断 tag 字段（如 auth/guard.ts、UserService）。未知则留空。",
        today = today_str,
        content = raw_text,
        modules = modules_section,
    );

    let config = crate::model_router::build_model_config(
        &settings.middle_model,
        &settings.middle_model_name,
        settings,
    )?;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()?;

    let router = ModelRouter::new();
    let response = rt
        .block_on(router.call_with_config(
            &config,
            &prompt,
            Some("你是 Auto-Heart，解析开发者工作计划。只输出 JSON 数组，不要 markdown 代码块。"),
        ))
        .ok()?;

    // 提取 JSON 数组并验证格式
    let json_str = extract_json_array(&response);
    serde_json::from_str::<serde_json::Value>(json_str).ok()?;
    Some(json_str.to_string())
}

/// 从 LLM 响应中提取 JSON 数组部分
fn extract_json_array(text: &str) -> &str {
    // 处理 ```json ... ``` 包装
    if let Some(start) = text.find("```json") {
        let inner = &text[start + 7..];
        if let Some(end) = inner.find("```") {
            return inner[..end].trim();
        }
    }
    // 找最外层 [ ... ]
    if let (Some(s), Some(e)) = (text.find('['), text.rfind(']')) {
        return &text[s..=e];
    }
    text
}

// ──────────────────────────────────────────────
// 行为信号辅助函数
// ──────────────────────────────────────────────

/// 检查最近 N 分钟内是否无任何文件活动（下班行为信号）
fn is_inactive_for_minutes(db: &DbPool, minutes: i64) -> bool {
    let db = match db.lock() {
        Ok(d) => d,
        Err(_) => return false,
    };
    let count: i32 = db
        .query_row(
            "SELECT COUNT(*) FROM file_changes WHERE timestamp > datetime('now', ?1)",
            rusqlite::params![format!("-{} minutes", minutes)],
            |row| row.get(0),
        )
        .unwrap_or(1);
    count == 0
}

// ──────────────────────────────────────────────
// 文件系统监听器
// ──────────────────────────────────────────────

/// 使用 notify crate 监听项目目录，将变更写入 file_changes 表
pub fn start_file_watcher(db: DbPool, watch_paths: Vec<PathBuf>) {
    thread::spawn(move || {
        let (tx, rx) = mpsc::channel::<notify::Result<Event>>();

        let mut watcher = match RecommendedWatcher::new(
            move |res| {
                let _ = tx.send(res);
            },
            Config::default().with_poll_interval(Duration::from_secs(2)),
        ) {
            Ok(w) => w,
            Err(e) => {
                eprintln!("[file_watcher] 创建失败: {}", e);
                return;
            }
        };

        for path in &watch_paths {
            if path.exists() {
                if let Err(e) = watcher.watch(path, RecursiveMode::Recursive) {
                    eprintln!("[file_watcher] 监听失败 {:?}: {}", path, e);
                }
            }
        }

        eprintln!("[file_watcher] 监听 {} 个目录", watch_paths.len());

        for event_result in rx {
            match event_result {
                Ok(event) => {
                    let change_type = match event.kind {
                        EventKind::Create(_) => "create",
                        EventKind::Modify(_) => "modify",
                        EventKind::Remove(_) => "delete",
                        _ => continue,
                    };

                    if let Ok(db) = db.lock() {
                        for path in &event.paths {
                            let path_str = path.to_string_lossy();
                            // 过滤无意义变更
                            if path_str.contains(".git")
                                || path_str.contains("node_modules")
                                || path_str.contains("target")
                                || path_str.contains(".lock")
                                || path_str.ends_with(".log")
                            {
                                continue;
                            }
                            let _ = db.execute(
                                "INSERT INTO file_changes (file_path, change_type) VALUES (?1, ?2)",
                                rusqlite::params![path_str.as_ref(), change_type],
                            );
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[file_watcher] 监听错误: {}", e);
                }
            }
        }
    });
}
