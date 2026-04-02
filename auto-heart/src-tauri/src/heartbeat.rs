use crate::database::DbPool;
use crate::model_router::ModelRouter;
use crate::settings::SettingsHandle;
use chrono::{NaiveDateTime, Timelike};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use rusqlite::OptionalExtension;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

pub type FileWatcherGeneration = Arc<AtomicU64>;

pub fn new_file_watcher_generation() -> FileWatcherGeneration {
    Arc::new(AtomicU64::new(0))
}

pub fn stop_file_watcher(watcher_generation: &FileWatcherGeneration) {
    watcher_generation.fetch_add(1, Ordering::SeqCst);
}

// ──────────────────────────────────────────────
// 操作日志 — 意图分析数据结构
// ──────────────────────────────────────────────

/// LLM 意图分析响应
#[derive(Deserialize)]
struct IntentionAnalysis {
    intentions: Vec<IntentionItem>,
}

#[derive(Deserialize)]
struct IntentionItem {
    file: String,
    change_type: String,
    description: String,
    confidence: f32,
    tags: Vec<String>,
}

/// 文件变更记录（用于批量分析）
#[derive(Clone)]
struct FileChangeRecord {
    path: String,
    change_type: String,
}

#[derive(Clone, Deserialize)]
struct ActiveWindowSnapshot {
    app_name: String,
    window_title: String,
}

// ──────────────────────────────────────────────
// 浅层心跳 — 每 30 秒（本地规则，0 token）
// ──────────────────────────────────────────────

/// 技术文档 §2.2：感知文件变更、活跃应用、意图文档、行为信号
pub fn start_shallow_heartbeat(app: AppHandle, db: DbPool, _settings: SettingsHandle) {
    thread::spawn(move || {
        let settings = _settings;
        let mut last_window: Option<ActiveWindowSnapshot> = None;
        let mut last_snapshot_at = Instant::now() - Duration::from_secs(301);
        let mut last_flush_check_at = Instant::now() - Duration::from_secs(10);
        let mut last_heartbeat_emit_at = Instant::now() - Duration::from_secs(31);

        loop {
            thread::sleep(Duration::from_secs(2));

            // 1. 活跃应用检测：窗口变化时立刻记录，同窗口则低频保活
            let active_window = get_active_window();
            if !active_window.app_name.is_empty() && active_window.app_name != "unknown" {
                let window_changed = last_window
                    .as_ref()
                    .map(|previous| has_meaningful_window_change(previous, &active_window))
                    .unwrap_or(true);
                let keepalive_due = !window_changed && last_snapshot_at.elapsed() >= Duration::from_secs(300);

                if window_changed || keepalive_due {
                    let source = if window_changed {
                        "foreground_change"
                    } else {
                        "foreground_keepalive"
                    };
                    log_activity_snapshot(&db, &active_window, source);
                    last_window = Some(active_window.clone());
                    last_snapshot_at = Instant::now();
                }
            }

            let intent_doc_path = settings
                .lock()
                .ok()
                .map(|snapshot| snapshot.intent_doc_path.clone())
                .unwrap_or_default();
            if !intent_doc_path.trim().is_empty() {
                check_intent_doc_update(&db, intent_doc_path.trim());
            }

            // 2. 自然节点检测：更高频检查，但只在满足条件时释放消息
            if last_flush_check_at.elapsed() >= Duration::from_secs(5) {
                let has_pending = check_and_flush_pending_messages(&app, &db);
                if has_pending {
                    let _ = app.emit("message_queue:flush", ());
                }
                last_flush_check_at = Instant::now();
            }

            if last_heartbeat_emit_at.elapsed() >= Duration::from_secs(30) {
                let _ = app.emit("heartbeat:shallow", ());
                last_heartbeat_emit_at = Instant::now();
            }
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
                "[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); $OutputEncoding = [Console]::OutputEncoding; try { (Get-Process | Where-Object {$_.MainWindowHandle -ne 0 -and $_.MainWindowTitle -ne ''} | Sort-Object CPU -Descending | Select-Object -First 1).ProcessName } catch { '' }",
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
fn get_active_window() -> ActiveWindowSnapshot {
    #[cfg(target_os = "windows")]
    {
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                r#"$signature = @"
using System;
using System.Runtime.InteropServices;
using System.Text;
  public static class Win32ForegroundWindow {
      [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
      [DllImport("user32.dll", SetLastError = true, CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);
      [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);
  }
  "@;
  Add-Type -TypeDefinition $signature -ErrorAction SilentlyContinue | Out-Null;
  [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false);
  $OutputEncoding = [Console]::OutputEncoding;
  try {
    $hwnd = [Win32ForegroundWindow]::GetForegroundWindow();
    if ($hwnd -eq [IntPtr]::Zero) { '{"app_name":"unknown","window_title":""}'; exit 0 }
  $buffer = New-Object System.Text.StringBuilder 1024;
  [void][Win32ForegroundWindow]::GetWindowText($hwnd, $buffer, $buffer.Capacity);
    [uint32]$targetPid = 0;
    [void][Win32ForegroundWindow]::GetWindowThreadProcessId($hwnd, [ref]$targetPid);
    $process = Get-Process -Id $targetPid -ErrorAction SilentlyContinue;
  [pscustomobject]@{
    app_name = if ($process) { $process.ProcessName } else { 'unknown' };
    window_title = $buffer.ToString().Trim();
  } | ConvertTo-Json -Compress
} catch {
  '{"app_name":"unknown","window_title":""}'
}"#,
            ])
            .output();
        return match output {
            Ok(o) => serde_json::from_str::<ActiveWindowSnapshot>(
                String::from_utf8_lossy(&o.stdout).trim(),
            )
            .unwrap_or(ActiveWindowSnapshot {
                app_name: "unknown".to_string(),
                window_title: String::new(),
            }),
            Err(_) => ActiveWindowSnapshot {
                app_name: "unknown".to_string(),
                window_title: String::new(),
            },
        };
    }

    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("osascript")
            .args([
                "-e",
                "tell application \"System Events\" to tell (first application process whose frontmost is true) to return name & \"||\" & front window's name",
            ])
            .output();
        return match output {
            Ok(o) => {
                let value = String::from_utf8_lossy(&o.stdout).trim().to_string();
                let mut parts = value.splitn(2, "||");
                ActiveWindowSnapshot {
                    app_name: parts.next().unwrap_or("unknown").trim().to_string(),
                    window_title: parts.next().unwrap_or("").trim().to_string(),
                }
            }
            Err(_) => ActiveWindowSnapshot {
                app_name: "unknown".to_string(),
                window_title: String::new(),
            },
        };
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        ActiveWindowSnapshot {
            app_name: "unknown".to_string(),
            window_title: String::new(),
        }
    }
}

fn classify_activity(app_name: &str, window_title: &str) -> &'static str {
    let combined = format!("{} {}", app_name.to_lowercase(), window_title.to_lowercase());

    if [
        "code", "cursor", "idea", "pycharm", "webstorm", "goland", "clion", "rustrover",
        "terminal", "powershell", "cmd", "git", "node", "python", "cargo",
    ]
    .iter()
    .any(|keyword| combined.contains(keyword))
    {
        "开发工作"
    } else if [
        "excel", "powerbi", "tableau", "dbeaver", "datagrip", "navicat", "jupyter",
        "notebook", "rstudio", "mysql", "sqlite",
    ]
    .iter()
    .any(|keyword| combined.contains(keyword))
    {
        "数据统计"
    } else if [
        "word", "notion", "obsidian", "typora", "xmind", "yuque", "语雀", "飞书文档",
    ]
    .iter()
    .any(|keyword| combined.contains(keyword))
    {
        "文档整理"
    } else if [
        "wechat", "weixin", "dingtalk", "feishu", "slack", "teams", "zoom", "meeting",
    ]
    .iter()
    .any(|keyword| combined.contains(keyword))
    {
        "沟通协作"
    } else if [
        "chrome", "msedge", "edge", "firefox", "safari", "github", "docs", "search",
        "stackoverflow",
    ]
    .iter()
    .any(|keyword| combined.contains(keyword))
    {
        "资料检索"
    } else if ["figma", "photoshop", "illustrator", "canva"].iter().any(|keyword| combined.contains(keyword)) {
        "设计创作"
    } else if ["explorer", "finder", "settings", "control panel"].iter().any(|keyword| combined.contains(keyword)) {
        "系统操作"
    } else {
        "其他操作"
    }
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_window_title(title: &str) -> String {
    collapse_whitespace(
        &title
            .replace('\u{200b}', " ")
            .replace('\u{feff}', " ")
            .replace('•', " ")
            .replace('●', " ")
            .replace('·', " ")
            .replace('|', " | "),
    )
    .trim()
    .to_lowercase()
}

fn split_title_segments(title: &str) -> Vec<String> {
    title
        .split(['-', '—', '|'])
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn has_meaningful_window_change(previous: &ActiveWindowSnapshot, current: &ActiveWindowSnapshot) -> bool {
    previous.app_name.to_lowercase() != current.app_name.to_lowercase()
        || normalize_window_title(&previous.window_title) != normalize_window_title(&current.window_title)
}

fn is_browser_app(app_name: &str) -> bool {
    matches!(
        app_name.to_lowercase().as_str(),
        "chrome" | "msedge" | "edge" | "firefox" | "safari" | "arc" | "zen"
    )
}

fn is_editor_app(app_name: &str) -> bool {
    matches!(
        app_name.to_lowercase().as_str(),
        "code"
            | "cursor"
            | "windsurf"
            | "code - insiders"
            | "idea64"
            | "pycharm64"
            | "webstorm64"
            | "goland64"
            | "clion64"
            | "rustrover64"
    )
}

fn summarize_browser_activity(title: &str) -> Option<String> {
    let segments = split_title_segments(title);
    let page = segments.first()?.trim();
    let site = segments.last().unwrap_or(&segments[0]).trim();
    let site_lower = site.to_lowercase();
    let page_lower = page.to_lowercase();

    let summary = if site_lower.contains("github") {
        format!("浏览 GitHub：{}", page)
    } else if site_lower.contains("gitlab") {
        format!("浏览 GitLab：{}", page)
    } else if site_lower.contains("gitee") {
        format!("浏览 Gitee：{}", page)
    } else if site_lower.contains("notion") || site_lower.contains("语雀") || site_lower.contains("yuque") {
        format!("查阅文档：{}", page)
    } else if site_lower.contains("google")
        || site_lower.contains("bing")
        || site_lower.contains("baidu")
        || page_lower.contains("search")
        || page_lower.contains("搜索")
    {
        format!("检索资料：{}", page)
    } else if page != site {
        format!("浏览 {}：{}", site, page)
    } else {
        format!("浏览网页：{}", page)
    };

    Some(summary)
}

fn collect_recent_file_paths(db: &DbPool, minutes_back: i64, limit: usize) -> Vec<String> {
    let db = match db.lock() {
        Ok(connection) => connection,
        Err(_) => return vec![],
    };
    let mut stmt = match db.prepare(
        "SELECT file_path
         FROM file_changes
         WHERE timestamp > datetime('now', ?1)
         ORDER BY timestamp DESC
         LIMIT ?2",
    ) {
        Ok(statement) => statement,
        Err(_) => return vec![],
    };

    stmt.query_map(
        rusqlite::params![format!("-{} minutes", minutes_back), limit as i64],
        |row| row.get::<_, String>(0),
    )
    .map(|rows| rows.filter_map(|row| row.ok()).collect())
    .unwrap_or_default()
}

fn collect_recent_operation_intentions(db: &DbPool, minutes_back: i64, limit: usize) -> Vec<String> {
    let db = match db.lock() {
        Ok(connection) => connection,
        Err(_) => return vec![],
    };
    let mut stmt = match db.prepare(
        "SELECT intention_desc
         FROM operation_log
         WHERE timestamp > datetime('now', ?1)
           AND intention_desc != ''
         ORDER BY timestamp DESC
         LIMIT ?2",
    ) {
        Ok(statement) => statement,
        Err(_) => return vec![],
    };

    stmt.query_map(
        rusqlite::params![format!("-{} minutes", minutes_back), limit as i64],
        |row| row.get::<_, String>(0),
    )
    .map(|rows| {
        rows.filter_map(|row| row.ok())
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

fn current_task_hint(db: &DbPool) -> Option<String> {
    let db = db.lock().ok()?;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let json_str: String = db
        .query_row(
            "SELECT parsed_tasks
             FROM intent_history
             WHERE date(created_at) = ?1 AND parsed_tasks != '[]'
             ORDER BY created_at DESC
             LIMIT 1",
            rusqlite::params![today],
            |row| row.get(0),
        )
        .ok()?;

    let tasks: serde_json::Value = serde_json::from_str(&json_str).ok()?;
    tasks
        .as_array()?
        .iter()
        .filter_map(|item| item.get("task").and_then(|value| value.as_str()))
        .map(str::trim)
        .find(|task| !task.is_empty())
        .map(ToOwned::to_owned)
}

fn dedupe_preserve_order(items: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    items.into_iter()
        .filter(|item| seen.insert(item.to_lowercase()))
        .collect()
}

fn infer_focus_from_branch(branch: &str) -> Option<String> {
    let normalized = branch.replace(['_', '-'], " ");
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return None;
    }

    let ignored = ["main", "master", "develop", "dev", "release", "staging", "production"];
    if ignored.iter().any(|item| trimmed.eq_ignore_ascii_case(item)) {
        return None;
    }

    let cleaned = trimmed
        .strip_prefix("feature/")
        .or_else(|| trimmed.strip_prefix("feat/"))
        .or_else(|| trimmed.strip_prefix("fix/"))
        .or_else(|| trimmed.strip_prefix("hotfix/"))
        .or_else(|| trimmed.strip_prefix("chore/"))
        .or_else(|| trimmed.strip_prefix("refactor/"))
        .unwrap_or(trimmed)
        .trim();

    if cleaned.is_empty() {
        None
    } else {
        Some(format!("推进 {}", cleaned))
    }
}

fn infer_focus_from_files(paths: &[String]) -> Option<String> {
    let labels = dedupe_preserve_order(
        paths.iter()
            .take(6)
            .filter_map(|path| {
                let normalized = path.replace('\\', "/");
                let parts: Vec<&str> = normalized.split('/').filter(|part| !part.is_empty()).collect();
                if parts.is_empty() {
                    return None;
                }

                if let Some(index) = parts.iter().position(|part| *part == "src") {
                    let after_src: Vec<&str> = parts.iter().skip(index + 1).take(2).copied().collect();
                    if !after_src.is_empty() {
                        return Some(after_src.join("/"));
                    }
                }

                if parts.len() >= 2 {
                    Some(format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1]))
                } else {
                    parts.last().map(|part| (*part).to_string())
                }
            })
            .collect(),
    );

    if labels.is_empty() {
        None
    } else if labels.len() == 1 {
        Some(format!("处理 {}", labels[0]))
    } else {
        Some(format!("围绕 {} 等模块开发", labels[..labels.len().min(2)].join("、")))
    }
}

fn shorten_path_label(path: &str) -> String {
    let path = path.replace('\\', "/");
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    if parts.len() <= 3 {
        return path;
    }

    parts[parts.len().saturating_sub(3)..].join("/")
}

fn find_git_repo_root(path: &Path) -> Option<PathBuf> {
    let mut current = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()?.to_path_buf()
    };

    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        current = current.parent()?.to_path_buf();
    }
}

fn current_git_branch(repo_root: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["-C", &repo_root.to_string_lossy(), "branch", "--show-current"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() {
        None
    } else {
        Some(branch)
    }
}

fn summarize_editor_activity(db: &DbPool, snapshot: &ActiveWindowSnapshot) -> Option<String> {
    let segments = split_title_segments(&snapshot.window_title);
    let mut meaningful = segments
        .into_iter()
        .filter(|segment| {
            let lower = segment.to_lowercase();
            !lower.contains("visual studio code")
                && !lower.contains("cursor")
                && !lower.contains("windsurf")
                && !lower.contains("administrator")
        })
        .collect::<Vec<_>>();

    let primary_label = if let Some(first) = meaningful.first() {
        first.clone()
    } else if snapshot.window_title.is_empty() {
        snapshot.app_name.clone()
    } else {
        snapshot.window_title.clone()
    };

    let workspace = if meaningful.len() >= 2 {
        meaningful.get(1).cloned()
    } else {
        None
    };

    let recent_files = collect_recent_file_paths(db, 12, 6);
    let recent_intentions = dedupe_preserve_order(collect_recent_operation_intentions(db, 20, 4));
    let file_hint = if recent_files.is_empty() {
        None
    } else {
        Some(
            recent_files
                .iter()
                .take(3)
                .map(|path| shorten_path_label(path))
                .collect::<Vec<_>>()
                .join("、"),
        )
    };
    let task_hint = current_task_hint(db);
    let inferred_file_focus = infer_focus_from_files(&recent_files);

    let branch = recent_files
        .iter()
        .find_map(|file| find_git_repo_root(Path::new(file)))
        .and_then(|repo_root| current_git_branch(&repo_root));
    let branch_focus = branch.as_ref().and_then(|item| infer_focus_from_branch(item));
    let active_focus = recent_intentions
        .first()
        .cloned()
        .or(task_hint)
        .or(branch_focus)
        .or(inferred_file_focus);

    let mut parts = vec![format!("编码开发：{}", primary_label)];
    if let Some(task) = active_focus {
        parts.push(format!("当前更像在实现“{}”", task));
    } else if let Some(workspace_name) = workspace {
        parts.push(format!("工作区 {}", workspace_name));
    }
    if let Some(files) = file_hint {
        parts.push(format!("最近改动 {}", files));
    }
    if let Some(branch_name) = branch {
        parts.push(format!("分支 {}", branch_name));
    }

    if recent_intentions.len() > 1 {
        parts.push(format!(
            "近期还在处理 {}",
            recent_intentions[1..recent_intentions.len().min(3)].join("、")
        ));
    }

    meaningful.clear();
    Some(parts.join(" | "))
}

fn build_activity_details(db: &DbPool, snapshot: &ActiveWindowSnapshot, category: &str) -> String {
    let app_name = snapshot.app_name.trim();
    let title = snapshot.window_title.trim();

    if is_editor_app(app_name) {
        if let Some(summary) = summarize_editor_activity(db, snapshot) {
            return summary;
        }
    }

    if is_browser_app(app_name) {
        if let Some(summary) = summarize_browser_activity(title) {
            return summary;
        }
    }

    if title.is_empty() {
        format!("{}：{}", category, app_name)
    } else {
        format!("{}：{} / {}", category, app_name, title)
    }
}

fn parse_db_time(value: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").ok()
}

fn build_work_session_signature(snapshot: &ActiveWindowSnapshot, category: &str) -> String {
    format!(
        "{}|{}|{}",
        snapshot.app_name.trim().to_lowercase(),
        category.trim().to_lowercase(),
        normalize_window_title(&snapshot.window_title),
    )
}

fn upsert_work_session(
    db: &rusqlite::Connection,
    snapshot: &ActiveWindowSnapshot,
    category: &str,
    source: &str,
    details: &str,
) {
    let signature = build_work_session_signature(snapshot, category);
    let latest_session = db
        .query_row(
            "SELECT id, signature, end_time
             FROM work_sessions
             WHERE date(start_time) = date('now')
             ORDER BY end_time DESC
             LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .ok()
        .flatten();

    if let Some((session_id, previous_signature, previous_end)) = latest_session {
        let can_merge = previous_signature == signature
            && parse_db_time(&previous_end)
                .map(|end_time| (chrono::Local::now().naive_local() - end_time).num_minutes() <= 5)
                .unwrap_or(false);

        if can_merge {
            let _ = db.execute(
                "UPDATE work_sessions
                 SET end_time = datetime('now'),
                     updated_at = datetime('now'),
                     summary = ?1,
                     source = ?2,
                     window_title = ?3
                 WHERE id = ?4",
                rusqlite::params![details, source, snapshot.window_title, session_id],
            );
            return;
        }
    }

    let _ = db.execute(
        "INSERT INTO work_sessions
         (id, app_name, window_title, category, summary, signature, source, start_time, end_time, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'), datetime('now'), datetime('now'))",
        rusqlite::params![
            Uuid::new_v4().to_string(),
            snapshot.app_name,
            snapshot.window_title,
            category,
            details,
            signature,
            source
        ],
    );
}

fn latest_open_work_session_id(db: &rusqlite::Connection) -> Option<String> {
    db.query_row(
        "SELECT id
         FROM work_sessions
         WHERE end_time > datetime('now', '-10 minutes')
         ORDER BY end_time DESC
         LIMIT 1",
        [],
        |row| row.get(0),
    )
    .optional()
    .ok()
    .flatten()
}

fn file_module_name(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let parts: Vec<&str> = normalized.split('/').filter(|part| !part.is_empty()).collect();
    if parts.len() >= 2 {
        parts[parts.len() - 2].to_string()
    } else if let Some(last) = parts.last() {
        (*last).to_string()
    } else {
        String::new()
    }
}

fn update_file_context(db: &rusqlite::Connection, file_path: &str, change_type: &str) {
    let project_goal: String = db
        .query_row(
            "SELECT current_goal
             FROM project_memory
             WHERE project_key = 'auto-heart'
             LIMIT 1",
            [],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten()
        .unwrap_or_default();
    let operation_hint: String = db
        .query_row(
            "SELECT intention_desc
             FROM operation_log
             WHERE file_path = ?1 AND intention_desc != ''
             ORDER BY timestamp DESC
             LIMIT 1",
            rusqlite::params![file_path],
            |row| row.get(0),
        )
        .optional()
        .ok()
        .flatten()
        .unwrap_or_default();
    let latest_task_hint = if !operation_hint.trim().is_empty() {
        operation_hint.trim().to_string()
    } else if !project_goal.trim().is_empty() {
        project_goal.trim().to_string()
    } else {
        String::new()
    };
    let related_session_id = latest_open_work_session_id(db);
    let latest_summary = if latest_task_hint.is_empty() {
        format!("最近发生 {} 变更", change_type)
    } else {
        format!("围绕“{}”发生 {} 变更", latest_task_hint, change_type)
    };

    let _ = db.execute(
        "INSERT INTO file_contexts
         (file_path, module_name, latest_summary, latest_task_hint, last_change_type, related_session_id, confidence, last_changed_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0.6, datetime('now'), datetime('now'))
         ON CONFLICT(file_path) DO UPDATE SET
            module_name = excluded.module_name,
            latest_summary = excluded.latest_summary,
            latest_task_hint = excluded.latest_task_hint,
            last_change_type = excluded.last_change_type,
            related_session_id = excluded.related_session_id,
            confidence = excluded.confidence,
            last_changed_at = datetime('now'),
            updated_at = datetime('now')",
        rusqlite::params![
            file_path,
            file_module_name(file_path),
            latest_summary,
            latest_task_hint,
            change_type,
            related_session_id
        ],
    );
}

fn log_activity_snapshot(db: &DbPool, snapshot: &ActiveWindowSnapshot, source: &str) {
    if let Ok(db) = db.lock() {
        let category = classify_activity(&snapshot.app_name, &snapshot.window_title);
        let duplicated: bool = db
            .query_row(
                "SELECT COUNT(*) FROM activity_snapshots
                 WHERE app_name = ?1
                   AND window_title = ?2
                   AND category = ?3
                   AND source = ?4
                   AND timestamp > datetime('now', '-10 seconds')",
                rusqlite::params![snapshot.app_name, snapshot.window_title, category, source],
                |row| row.get::<_, i32>(0),
            )
            .map(|count| count > 0)
            .unwrap_or(false);

        if duplicated {
            return;
        }
    }

    let category = classify_activity(&snapshot.app_name, &snapshot.window_title);
    let details = build_activity_details(db, snapshot, category);

    if let Ok(db) = db.lock() {
        let _ = db.execute(
            "INSERT INTO activity_snapshots (app_name, window_title, category, source, details)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![snapshot.app_name, snapshot.window_title, category, source, details],
        );
        upsert_work_session(&db, snapshot, category, source, &details);

        let _ = db.execute(
            "INSERT INTO decision_log (id, description, reason, related_file, context) \
             VALUES (?1, 'active_app_log', ?2, '', datetime('now'))",
            rusqlite::params![
                Uuid::new_v4().to_string(),
                format!("active:{} [{}] {}", snapshot.app_name, category, details)
            ],
        );
    }
}

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

            if let Some(report) = generate_daily_report_v2(&db, &settings_snap, &today) {
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
fn generate_daily_report_v2(
    db: &DbPool,
    settings: &crate::settings::AppSettings,
    date: &str,
) -> Option<String> {
    let file_changes = {
        let db = db.lock().ok()?;
        let mut stmt = db
            .prepare("SELECT DISTINCT file_path FROM file_changes WHERE date(timestamp) = ?1 LIMIT 20")
            .ok()?;
        let rows = stmt.query_map(rusqlite::params![date], |row| row.get::<_, String>(0))
            .ok()?
            .filter_map(|r| r.ok())
            .collect::<Vec<_>>();
        rows
    };

    let semantic_updates = {
        let db = db.lock().ok()?;
        let mut stmt = db
            .prepare("SELECT module_name, understanding FROM semantic_modules WHERE date(updated_at) = ?1")
            .ok()?;
        let rows = stmt.query_map(rusqlite::params![date], |row| {
            Ok(format!(
                "{}: {}",
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?
            ))
        })
        .ok()?
        .filter_map(|r| r.ok())
        .collect::<Vec<_>>();
        rows
    };

    let activity_summary = {
        let db = db.lock().ok()?;
        let mut stmt = db
            .prepare(
                "SELECT category, COUNT(*) AS cnt
                 FROM activity_snapshots
                 WHERE date(timestamp) = ?1
                 GROUP BY category
                 ORDER BY cnt DESC, category ASC
                 LIMIT 6",
            )
            .ok()?;
        let rows = stmt.query_map(rusqlite::params![date], |row| {
            let count: i32 = row.get(1)?;
            Ok(format!("{}: 约{}分钟", row.get::<_, String>(0)?, count * 30))
        })
        .ok()?
        .filter_map(|r| r.ok())
        .collect::<Vec<_>>();
        rows
    };

    if file_changes.is_empty() && semantic_updates.is_empty() && activity_summary.is_empty() {
        return None;
    }

    let prompt = format!(
        "今天是 {}。\n\n涉及文件：\n{}\n\n语义理解：\n{}\n\n电脑上的工作活动：\n{}\n\n请生成一份有业务意义的工作日报（200字以内）。\n要求：\n- 结合开发、数据、文档、沟通等活动，总结今天真实完成的工作\n- 体现业务价值，而不是简单罗列文件名\n- 指出明日值得继续推进或关注的事项",
        date,
        if file_changes.is_empty() { "暂无文件变更".to_string() } else { file_changes.join("\n") },
        if semantic_updates.is_empty() { "暂无语义更新".to_string() } else { semantic_updates.join("\n") },
        if activity_summary.is_empty() { "暂无前台活动快照".to_string() } else { activity_summary.join("\n") }
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
        Some("你是 Auto-Heart，一名理解开发者全天工作上下文的 AI 助理。请输出简洁、业务导向、可直接发送的中文日报。"),
    ))
    .ok()
}

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

pub fn parse_pending_intents_now(
    db: &DbPool,
    app: &AppHandle,
    settings: &crate::settings::AppSettings,
) -> bool {
    let before_count = {
        let Ok(db) = db.lock() else {
            return false;
        };
        db.query_row(
            "SELECT COUNT(*) FROM intent_history WHERE parsed_tasks != '[]' AND date(created_at) = date('now')",
            [],
            |row| row.get::<_, i32>(0),
        )
        .unwrap_or(0)
    };

    parse_pending_intent_docs(db, app, settings);

    let after_count = {
        let Ok(db) = db.lock() else {
            return false;
        };
        db.query_row(
            "SELECT COUNT(*) FROM intent_history WHERE parsed_tasks != '[]' AND date(created_at) = date('now')",
            [],
            |row| row.get::<_, i32>(0),
        )
        .unwrap_or(0)
    };

    after_count > before_count
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
// 操作日志 — 意图分析
// ──────────────────────────────────────────────

/// 收集近 N 分钟的文件变更
fn collect_recent_file_changes(db: &DbPool, minutes_back: i64) -> Vec<FileChangeRecord> {
    let db = match db.lock() {
        Ok(d) => d,
        Err(_) => return vec![],
    };
    let param = format!("-{} minutes", minutes_back);
    let mut stmt = match db.prepare(
        "SELECT file_path, change_type FROM file_changes \
         WHERE timestamp > datetime('now', ?1) LIMIT 50",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    stmt.query_map(rusqlite::params![param], |row| {
        Ok(FileChangeRecord {
            path: row.get(0)?,
            change_type: row.get(1)?,
        })
    })
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

/// 调用 LLM 分析文件变更意图
fn call_intention_analysis(
    settings: &crate::settings::AppSettings,
    changes: &[FileChangeRecord],
) -> Option<IntentionAnalysis> {
    if changes.is_empty() {
        return None;
    }

    let changes_text = changes
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{}. {} [{}]", i + 1, c.path, c.change_type))
        .collect::<Vec<_>>()
        .join("\n");

    let prompt = format!(
        "以下是你监测到的文件变更（每5分钟收集一次）：\n{}\n\n\
        请分析这些变更的意图，输出 JSON（无其他内容）：\n\
        {{\"intentions\": [{{\"file\":\"路径\",\"change_type\":\"类型\",\"description\":\"意图描述（中文，简洁，20字内）\",\"confidence\":0.8,\"tags\":[\"标签\"]}}]}}\n\n\
        标签从以下选择：feature, bugfix, refactor, docs, chore, security, performance\n\
        confidence 0.0~1.0，过低（<0.5）的分析结果不写入\n\
        只输出 JSON，不要 markdown 代码块。",
        changes_text
    );

    let config = match crate::model_router::build_model_config(
        &settings.middle_model,
        &settings.middle_model_name,
        settings,
    ) {
        Some(c) => c,
        None => return None,
    };

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()?;
    let router = ModelRouter::new();
    let response = rt.block_on(router.call_with_config(
        &config,
        &prompt,
        Some("你是 Auto-Heart 的意图分析助手，只输出 JSON。"),
    )).ok()?;

    let json_str = extract_json(&response);
    serde_json::from_str(json_str).ok()
}

/// 保存意图分析结果到 operation_log 表
fn save_operation_logs(db: &DbPool, analysis: &IntentionAnalysis, chunk_id: &str) {
    let db = match db.lock() {
        Ok(d) => d,
        Err(_) => return,
    };

    for item in &analysis.intentions {
        if item.confidence < 0.5 {
            continue;
        }
        let id = Uuid::new_v4().to_string();
        let tags_json = serde_json::to_string(&item.tags).unwrap_or_default();
        let _ = db.execute(
            "INSERT INTO operation_log \
             (id, file_path, change_type, intention_desc, confidence, tags, chunk_id) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                id,
                item.file,
                item.change_type,
                item.description,
                item.confidence,
                tags_json,
                chunk_id
            ],
        );
    }
}

// ──────────────────────────────────────────────
// 主动建议引擎
// ──────────────────────────────────────────────

/// 发送主动建议气泡
fn emit_agent_alert(app: &AppHandle, alert_type: &str, title: &str, message: &str) {
    eprintln!("[agent] alert: {} - {}", alert_type, title);
    let _ = app.emit(
        "agent:alert",
        serde_json::json!({
            "type": alert_type,
            "title": title,
            "message": message,
        }),
    );
}

/// 检测关键模块变更
fn detect_critical_changes(
    changes: &[FileChangeRecord],
    keywords: &[String],
) -> Vec<FileChangeRecord> {
    changes
        .iter()
        .filter(|c| {
            let path_lower = c.path.to_lowercase();
            keywords.iter().any(|kw| path_lower.contains(&kw.to_lowercase()))
        })
        .cloned()
        .collect()
}

/// 获取关键模块关键词列表
fn get_critical_keywords(settings: &crate::settings::AppSettings) -> Vec<String> {
    settings
        .critical_keywords
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// 检查并触发主动建议
fn check_proactive_suggestions(
    app: &AppHandle,
    db: &DbPool,
    settings: &crate::settings::AppSettings,
    recent_changes: &[FileChangeRecord],
) {
    if !settings.proactive_suggestions {
        return;
    }

    // 1. 关键模块变更检测
    let keywords = get_critical_keywords(settings);
    let critical = detect_critical_changes(recent_changes, &keywords);
    if !critical.is_empty() {
        let files: Vec<&str> = critical.iter().map(|c| c.path.as_str()).collect();
        emit_agent_alert(
            app,
            "critical",
            "关键模块变更",
            &format!("检测到关键文件变更：{}", files.join(", ")),
        );
        return;
    }

    // 2. intent 文档今日是否更新
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let intent_updated: bool = {
        let db = match db.lock() {
            Ok(d) => d,
            Err(_) => return,
        };
        let count: i32 = db
            .query_row(
                "SELECT COUNT(*) FROM intent_history \
                 WHERE date(created_at) = ?1 AND parsed_tasks != '[]'",
                rusqlite::params![today],
                |row| row.get::<_, i32>(0),
            )
            .unwrap_or(0);
        count > 0
    };

    if !intent_updated {
        emit_agent_alert(
            app,
            "intent_reminder",
            "今日意图待更新",
            "你今天还没有更新意图文档，要看一下吗？",
        );
    }
}

// ──────────────────────────────────────────────
// 操作日志心跳 — 每 5 分钟（意图分析）
// ──────────────────────────────────────────────

pub fn start_operation_log_heartbeat(
    app: AppHandle,
    db: DbPool,
    settings: SettingsHandle,
) {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(300)); // 5 分钟

            let changes = collect_recent_file_changes(&db, 5);
            if changes.is_empty() {
                continue;
            }

            let chunk_id = Uuid::new_v4().to_string();
            let settings_snap = { settings.lock().unwrap().clone() };

            if let Some(analysis) = call_intention_analysis(&settings_snap, &changes) {
                eprintln!("[operation_log] 分析了 {} 项变更", analysis.intentions.len());
                save_operation_logs(&db, &analysis, &chunk_id);
            }
            // 主动建议检查
            check_proactive_suggestions(&app, &db, &settings_snap, &changes);
        }
    });
}

// ──────────────────────────────────────────────
// 文件系统监听器
// ──────────────────────────────────────────────

/// 使用 notify crate 监听项目目录，将变更写入 file_changes 表
pub fn start_file_watcher(
    db: DbPool,
    watch_paths: Vec<PathBuf>,
    watcher_generation: FileWatcherGeneration,
) {
    let current_generation = watcher_generation.fetch_add(1, Ordering::SeqCst) + 1;
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

        loop {
            if watcher_generation.load(Ordering::SeqCst) != current_generation {
                eprintln!("[file_watcher] 停止旧监听器 generation={}", current_generation);
                break;
            }

            let event_result = match rx.recv_timeout(Duration::from_secs(1)) {
                Ok(event) => event,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            };

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
                            update_file_context(&db, path_str.as_ref(), change_type);
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
