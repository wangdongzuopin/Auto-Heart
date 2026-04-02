use crate::database::DbPool;
use crate::heartbeat::{start_file_watcher, stop_file_watcher, FileWatcherGeneration};
use crate::settings::{AppSettings, SettingsHandle, WindowState};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
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

fn load_latest_today_task_row(
    db: &rusqlite::Connection,
) -> Option<(String, String, String)> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    db.query_row(
        "SELECT id, raw_text, parsed_tasks
         FROM intent_history
         WHERE date(created_at) = ?1
         ORDER BY created_at DESC
         LIMIT 1",
        rusqlite::params![today],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        },
    )
    .ok()
}

fn update_latest_today_tasks(
    db: &rusqlite::Connection,
    tasks: &[TodayTask],
) -> Result<(), String> {
    let Some((row_id, _, _)) = load_latest_today_task_row(db) else {
        return Err("today tasks not found".to_string());
    };

    db.execute(
        "UPDATE intent_history
         SET parsed_tasks = ?1,
             completion_status = 'active'
         WHERE id = ?2",
        rusqlite::params![
            serde_json::to_string(tasks).map_err(|error| error.to_string())?,
            row_id
        ],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

#[derive(Serialize, Clone)]
pub struct ActivityCategoryStat {
    pub category: String,
    pub count: i32,
    pub minutes: i32,
}

#[derive(Serialize, Clone)]
pub struct ActivitySnapshotEntry {
    pub app_name: String,
    pub window_title: String,
    pub category: String,
    pub details: String,
    pub timestamp: String,
}

#[derive(Serialize, Clone)]
pub struct ActivitySessionStat {
    pub label: String,
    pub category: String,
    pub start_time: String,
    pub end_time: String,
    pub minutes: i64,
}

#[derive(Serialize, Clone)]
pub struct TodayActivitySummary {
    pub total_active_minutes: i64,
    pub total_idle_minutes: i64,
    pub context_switches: i32,
    pub categories: Vec<ActivityCategoryStat>,
    pub sessions: Vec<ActivitySessionStat>,
    pub snapshots: Vec<ActivitySnapshotEntry>,
}

#[derive(Serialize, Clone)]
pub struct GitCommitEntry {
    pub repo_path: String,
    pub short_hash: String,
    pub summary: String,
    pub committed_at: String,
}

#[derive(Serialize, Clone)]
pub struct TrackingHealth {
    pub current_db_path: String,
    pub watch_paths: Vec<String>,
    pub repo_paths: Vec<String>,
    pub today_activity_snapshots: i32,
    pub today_file_changes: i32,
    pub today_operation_logs: i32,
    pub today_git_commits: i32,
    pub latest_activity_at: Option<String>,
    pub latest_file_change_at: Option<String>,
    pub latest_git_commit: Option<GitCommitEntry>,
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
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
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

fn parse_db_timestamp(value: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").ok()
}

fn empty_activity_summary() -> TodayActivitySummary {
    TodayActivitySummary {
        total_active_minutes: 0,
        total_idle_minutes: 0,
        context_switches: 0,
        categories: vec![],
        sessions: vec![],
        snapshots: vec![],
    }
}

fn default_tracking_health(watch_paths: Vec<String>) -> TrackingHealth {
    TrackingHealth {
        current_db_path: String::new(),
        watch_paths,
        repo_paths: vec![],
        today_activity_snapshots: 0,
        today_file_changes: 0,
        today_operation_logs: 0,
        today_git_commits: 0,
        latest_activity_at: None,
        latest_file_change_at: None,
        latest_git_commit: None,
    }
}

#[derive(Clone)]
struct TrackingHealthCacheEntry {
    key: String,
    value: TrackingHealth,
    cached_at: Instant,
}

fn tracking_health_cache() -> &'static Mutex<Option<TrackingHealthCacheEntry>> {
    static CACHE: OnceLock<Mutex<Option<TrackingHealthCacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn is_ignored_dir(name: &str) -> bool {
    matches!(
        name,
        ".git" | "node_modules" | "target" | "dist" | "build" | ".next" | ".turbo" | ".idea"
    )
}

fn discover_git_repos(root: &Path, depth: usize, repos: &mut BTreeSet<String>) {
    if depth == 0 || !root.exists() || !root.is_dir() {
        return;
    }

    if root.join(".git").exists() {
        repos.insert(root.to_string_lossy().to_string());
        return;
    }

    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if is_ignored_dir(name) {
            continue;
        }
        discover_git_repos(&path, depth - 1, repos);
    }
}

fn collect_repo_paths(watch_paths: &[String]) -> Vec<String> {
    let mut repos = BTreeSet::new();

    for watch_path in watch_paths {
        let root = PathBuf::from(watch_path);
        discover_git_repos(&root, 3, &mut repos);
    }

    repos.into_iter().collect()
}

fn collect_today_git_commits(repo_paths: &[String]) -> Vec<GitCommitEntry> {
    let mut commits = Vec::new();

    for repo_path in repo_paths {
        let output = Command::new("git")
            .args([
                "-C",
                repo_path,
                "log",
                "--since=midnight",
                "--pretty=format:%h\x1f%ad\x1f%s",
                "--date=format:%Y-%m-%d %H:%M:%S",
                "--max-count",
                "12",
            ])
            .output();

        let Ok(output) = output else {
            continue;
        };
        if !output.status.success() {
            continue;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let parts: Vec<&str> = line.split('\x1f').collect();
            if parts.len() != 3 {
                continue;
            }
            commits.push(GitCommitEntry {
                repo_path: repo_path.clone(),
                short_hash: parts[0].trim().to_string(),
                committed_at: parts[1].trim().to_string(),
                summary: parts[2].trim().to_string(),
            });
        }
    }

    commits.sort_by(|left, right| right.committed_at.cmp(&left.committed_at));
    commits
}

fn build_today_activity_summary(db: &rusqlite::Connection) -> TodayActivitySummary {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let work_sessions = db
        .prepare(
            "SELECT app_name, window_title, category, summary, start_time, end_time
             FROM work_sessions
             WHERE date(start_time) = ?1
             ORDER BY start_time ASC",
        )
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map(rusqlite::params![today.clone()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            })
            .ok()
            .map(|rows| rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
        })
        .unwrap_or_default();

    let timeline = db
        .prepare(
            "SELECT app_name, window_title, category, details, timestamp
             FROM activity_snapshots
             WHERE date(timestamp) = ?1
             ORDER BY timestamp ASC",
        )
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map(rusqlite::params![today], |row| {
                Ok(ActivitySnapshotEntry {
                    app_name: row.get(0)?,
                    window_title: row.get(1)?,
                    category: row.get(2)?,
                    details: row.get(3)?,
                    timestamp: row.get(4)?,
                })
            })
            .ok()
            .map(|rows| rows.filter_map(|r| r.ok()).collect::<Vec<_>>())
        })
        .unwrap_or_default();

    let mut total_active_minutes = 0_i64;
    let mut total_idle_minutes = 0_i64;
    let mut context_switches = 0_i32;
    let mut sessions: Vec<ActivitySessionStat> = vec![];
    let mut category_minutes: BTreeMap<String, i64> = BTreeMap::new();
    let mut category_counts: BTreeMap<String, i32> = BTreeMap::new();

    if !work_sessions.is_empty() {
        let mut previous_end: Option<NaiveDateTime> = None;
        let mut previous_category: Option<String> = None;
        let mut previous_app: Option<String> = None;

        for (app_name, window_title, category, summary, start_time, end_time) in &work_sessions {
            let (Some(start_ts), Some(end_ts)) =
                (parse_db_timestamp(start_time), parse_db_timestamp(end_time))
            else {
                continue;
            };

            if let Some(previous_end_ts) = previous_end {
                let gap_minutes = (start_ts - previous_end_ts).num_minutes().max(0);
                if gap_minutes >= 5 {
                    total_idle_minutes += gap_minutes;
                }
            }

            if let (Some(previous_category_value), Some(previous_app_value)) =
                (previous_category.as_ref(), previous_app.as_ref())
            {
                if previous_category_value != category || previous_app_value != app_name {
                    context_switches += 1;
                }
            }

            let minutes = (end_ts - start_ts).num_minutes().max(1);
            total_active_minutes += minutes;
            *category_minutes.entry(category.clone()).or_insert(0) += minutes;
            *category_counts.entry(category.clone()).or_insert(0) += 1;

            let label = if !summary.trim().is_empty() {
                summary.clone()
            } else if window_title.is_empty() {
                app_name.clone()
            } else {
                format!("{} · {}", app_name, window_title)
            };

            sessions.push(ActivitySessionStat {
                label,
                category: category.clone(),
                start_time: start_time[11..16].to_string(),
                end_time: end_time[11..16].to_string(),
                minutes,
            });

            previous_end = Some(end_ts);
            previous_category = Some(category.clone());
            previous_app = Some(app_name.clone());
        }
    }

    let mut categories = category_minutes
        .into_iter()
        .map(|(category, minutes)| ActivityCategoryStat {
            count: category_counts.get(&category).copied().unwrap_or(0),
            category,
            minutes: minutes as i32,
        })
        .collect::<Vec<_>>();
    categories.sort_by(|left, right| {
        right
            .minutes
            .cmp(&left.minutes)
            .then_with(|| left.category.cmp(&right.category))
    });

    sessions.sort_by(|left, right| right.minutes.cmp(&left.minutes));
    let snapshots = timeline.iter().rev().take(12).cloned().collect::<Vec<_>>();

    TodayActivitySummary {
        total_active_minutes,
        total_idle_minutes,
        context_switches,
        categories: categories.into_iter().take(6).collect(),
        sessions: sessions.into_iter().take(6).collect(),
        snapshots,
    }
}

#[tauri::command]
pub fn get_today_activity_summary(db: State<'_, DbPool>) -> TodayActivitySummary {
    let db = match db.lock() {
        Ok(d) => d,
        Err(_) => return empty_activity_summary(),
    };

    build_today_activity_summary(&db)
}

#[tauri::command]
pub fn clear_today_activity_snapshots(db: State<'_, DbPool>) -> Result<(), String> {
    let db = db.lock().map_err(|error| error.to_string())?;
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    db.execute(
        "DELETE FROM activity_snapshots WHERE date(timestamp) = ?1",
        rusqlite::params![today],
    )
    .map_err(|error| error.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn get_tracking_health(
    app: AppHandle,
    settings: State<'_, SettingsHandle>,
    db: State<'_, DbPool>,
) -> TrackingHealth {
    let settings_snapshot = settings.lock().unwrap().clone();
    let watch_paths = if settings_snapshot.watch_paths.is_empty() {
        crate::settings::auto_detect_watch_paths()
    } else {
        settings_snapshot.watch_paths.clone()
    };

    let mut health = default_tracking_health(watch_paths.clone());
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let base_dir = if settings_snapshot.data_dir.is_empty() {
        app.path().app_data_dir().unwrap_or_default()
    } else {
        PathBuf::from(&settings_snapshot.data_dir)
    };
    let cache_key = format!(
        "{}|{}|{}",
        today,
        base_dir.to_string_lossy(),
        watch_paths.join(";")
    );
    if let Ok(cache) = tracking_health_cache().lock() {
        if let Some(entry) = cache.as_ref() {
            if entry.key == cache_key && entry.cached_at.elapsed() < Duration::from_secs(5) {
                return entry.value.clone();
            }
        }
    }
    health.current_db_path = base_dir
        .join(&today)
        .join("auto_heart.db")
        .to_string_lossy()
        .to_string();

    let repo_paths = collect_repo_paths(&watch_paths);
    let git_commits = collect_today_git_commits(&repo_paths);
    health.today_git_commits = git_commits.len() as i32;
    health.latest_git_commit = git_commits.first().cloned();
    health.repo_paths = repo_paths;

    let Ok(db) = db.lock() else {
        return health;
    };

    health.today_activity_snapshots = db
        .query_row(
            "SELECT COUNT(*) FROM activity_snapshots WHERE date(timestamp) = ?1",
            rusqlite::params![today.clone()],
            |row| row.get(0),
        )
        .unwrap_or(0);
    health.today_file_changes = db
        .query_row(
            "SELECT COUNT(*) FROM file_changes WHERE date(timestamp) = ?1",
            rusqlite::params![today.clone()],
            |row| row.get(0),
        )
        .unwrap_or(0);
    health.today_operation_logs = db
        .query_row(
            "SELECT COUNT(*) FROM operation_log WHERE date(timestamp) = ?1",
            rusqlite::params![today.clone()],
            |row| row.get(0),
        )
        .unwrap_or(0);
    health.latest_activity_at = db
        .query_row(
            "SELECT timestamp FROM activity_snapshots ORDER BY timestamp DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok();
    health.latest_file_change_at = db
        .query_row(
            "SELECT timestamp FROM file_changes ORDER BY timestamp DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .ok();

    if let Ok(mut cache) = tracking_health_cache().lock() {
        *cache = Some(TrackingHealthCacheEntry {
            key: cache_key,
            value: health.clone(),
            cached_at: Instant::now(),
        });
    }

    health
}
fn should_use_local_activity_summary(content: &str) -> bool {
    let normalized: String = content.chars().filter(|c| !c.is_whitespace()).collect();
    let has_today = normalized.contains("\u{4eca}\u{5929}")
        || normalized.contains("\u{4eca}\u{65e5}")
        || normalized.contains("\u{672c}\u{65e5}");
    let has_summary = normalized.contains("\u{603b}\u{7ed3}")
        || normalized.contains("\u{6c47}\u{603b}")
        || normalized.contains("\u{65e5}\u{62a5}")
        || normalized.contains("\u{62a5}\u{544a}")
        || normalized.contains("\u{56de}\u{987e}");
    let has_local = normalized.contains("\u{76d1}\u{542c}")
        || normalized.contains("\u{672c}\u{5730}")
        || normalized.contains("\u{6587}\u{4ef6}")
        || normalized.contains("\u{8bb0}\u{5f55}");
    let asks_what_i_did = normalized.contains("\u{6211}\u{90fd}\u{5e72}\u{4e86}\u{4ec0}\u{4e48}")
        || normalized.contains("\u{4eca}\u{5929}\u{505a}\u{4e86}\u{4ec0}\u{4e48}")
        || normalized.contains("\u{4eca}\u{5929}\u{90fd}\u{505a}\u{4e86}\u{4ec0}\u{4e48}");

    (has_today && has_summary) || (has_local && has_summary) || asks_what_i_did
}

fn wants_five_point_report(content: &str) -> bool {
    let normalized: String = content.chars().filter(|c| !c.is_whitespace()).collect();
    let has_report = normalized.contains("\u{65e5}\u{62a5}")
        || normalized.contains("\u{603b}\u{7ed3}")
        || normalized.contains("\u{6c47}\u{603b}")
        || normalized.contains("\u{56de}\u{987e}");
    let has_five_points = normalized.contains("\u{4e94}\u{70b9}")
        || normalized.contains("5\u{70b9}")
        || normalized.contains("\u{4e94}\u{6761}")
        || normalized.contains("5\u{6761}")
        || normalized.contains("\u{4e94}\u{9879}")
        || normalized.contains("5\u{9879}");

    has_report && has_five_points
}

fn format_minutes(minutes: i64) -> String {
    if minutes >= 60 {
        let hours = minutes / 60;
        let remain = minutes % 60;
        if remain == 0 {
            format!("{} 小时", hours)
        } else {
            format!("{} 小时 {} 分钟", hours, remain)
        }
    } else {
        format!("{} 分钟", minutes)
    }
}

fn shorten_file_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
        .unwrap_or_else(|| path.to_string())
}

fn build_local_report_from_data(
    content: &str,
    db: &DbPool,
    settings: &AppSettings,
) -> Option<String> {
    let db = db.lock().ok()?;
    let summary = build_today_activity_summary(&db);
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    let file_changes = db
        .prepare(
            "SELECT file_path, change_type, timestamp
             FROM file_changes
             WHERE date(timestamp) = ?1
             ORDER BY timestamp DESC
             LIMIT 20",
        )
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map(rusqlite::params![today.clone()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .ok()
            .map(|rows| rows.filter_map(|row| row.ok()).collect::<Vec<_>>())
        })
        .unwrap_or_default();

    let operation_logs = db
        .prepare(
            "SELECT intention_desc, file_path, timestamp
             FROM operation_log
             WHERE date(timestamp) = ?1
             ORDER BY timestamp DESC
             LIMIT 10",
        )
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map(rusqlite::params![today.clone()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .ok()
            .map(|rows| rows.filter_map(|row| row.ok()).collect::<Vec<_>>())
        })
        .unwrap_or_default();

    let existing_report: Option<String> = db
        .query_row(
            "SELECT content FROM daily_reports WHERE date = ?1 ORDER BY created_at DESC LIMIT 1",
            rusqlite::params![today.clone()],
            |row| row.get(0),
        )
        .ok();

    let watch_paths = if settings.watch_paths.is_empty() {
        crate::settings::auto_detect_watch_paths()
    } else {
        settings.watch_paths.clone()
    };
    let repo_paths = collect_repo_paths(&watch_paths);
    let git_commits = collect_today_git_commits(&repo_paths);

    if summary.snapshots.is_empty()
        && file_changes.is_empty()
        && operation_logs.is_empty()
        && existing_report.is_none()
        && git_commits.is_empty()
    {
        return None;
    }

    if wants_five_point_report(content) {
        let mut bullets: Vec<String> = Vec::new();

        if let Some(first) = summary.categories.first() {
            bullets.push(format!(
                "1. 今天主要时间集中在{}，累计约{}。",
                first.category,
                format_minutes(first.minutes as i64)
            ));
        } else {
            bullets.push("1. 今天已经开始有本地工作记录，但分类时长还不够完整。".to_string());
        }

        if let Some(commit) = git_commits.first() {
            bullets.push(format!(
                "2. 代码侧已有提交记录，最近一次提交是 {}（{}）。",
                commit.summary, commit.short_hash
            ));
        } else if let Some(session) = summary.sessions.first() {
            bullets.push(format!(
                "2. 最长连续工作段在 {} - {}，主要处理“{}”，持续约{}。",
                session.start_time,
                session.end_time,
                session.label,
                format_minutes(session.minutes)
            ));
        }

        if !file_changes.is_empty() {
            let files = file_changes
                .iter()
                .take(3)
                .map(|(file_path, _, _)| shorten_file_path(file_path))
                .collect::<Vec<_>>()
                .join("、");
            bullets.push(format!("3. 本地文件有持续修改，最近涉及：{}。", files));
        } else {
            bullets.push("3. 今天暂时还没有采集到明确的文件修改记录。".to_string());
        }

        if !summary.snapshots.is_empty() {
            let docs = summary
                .snapshots
                .iter()
                .filter(|item| {
                    let app = item.app_name.to_lowercase();
                    app.contains("wps") || app.contains("chrome") || app.contains("edge")
                })
                .take(2)
                .map(|item| format!("{}：{}", item.app_name, item.window_title))
                .collect::<Vec<_>>();
            if !docs.is_empty() {
                bullets.push(format!("4. 文档和浏览器侧的主要操作包括：{}。", docs.join("；")));
            }
        } else {
            bullets.push(format!(
                "4. 今天发生了 {} 次上下文切换，说明工作在不同窗口和任务间来回切换。",
                summary.context_switches
            ));
        }

        if !operation_logs.is_empty() {
            let intentions = operation_logs
                .iter()
                .take(2)
                .map(|(intention, _, _)| intention.clone())
                .collect::<Vec<_>>()
                .join("；");
            bullets.push(format!("5. 从本地操作日志判断，今天的工作重点包括：{}。", intentions));
        } else if let Some(report) = existing_report {
            let preview = report
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("已有日报草稿");
            bullets.push(format!("5. 系统里已经有一份日报草稿，可继续润色：{}。", preview));
        } else {
            bullets.push("5. 这些记录已经足够继续整理成正式日报，可以让我继续改写成对外发送版本。".to_string());
        }

        let mut lines = vec![
            "<think>".to_string(),
            "我优先读取了今天的本地活动快照、文件变更、操作日志、Git 提交和已有日报草稿，再按日报口径整理成五点总结。".to_string(),
            "</think>".to_string(),
            String::new(),
            "基于今天的本地记录，日报可以先总结为这五点：".to_string(),
        ];
        lines.extend(bullets);
        return Some(lines.join("\n"));
    }

    None
}

fn build_local_activity_chat_reply(db: &DbPool) -> String {
    let db = match db.lock() {
        Ok(d) => d,
        Err(_) => {
            return "<think>\n我尝试读取本地监听数据库，但数据库当前不可用，所以没法生成准确摘要。\n</think>\n\n现在暂时拿不到监听数据，请稍后再试。".to_string();
        }
    };

    let summary = build_today_activity_summary(&db);
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    let file_changes = db
        .prepare(
            "SELECT file_path, change_type, timestamp
             FROM file_changes
             WHERE date(timestamp) = ?1
             ORDER BY timestamp DESC
             LIMIT 8",
        )
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map(rusqlite::params![today.clone()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .ok()
            .map(|rows| rows.filter_map(|row| row.ok()).collect::<Vec<_>>())
        })
        .unwrap_or_default();

    let operation_logs = db
        .prepare(
            "SELECT intention_desc, file_path, timestamp
             FROM operation_log
             WHERE date(timestamp) = ?1
             ORDER BY timestamp DESC
             LIMIT 5",
        )
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map(rusqlite::params![today.clone()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .ok()
            .map(|rows| rows.filter_map(|row| row.ok()).collect::<Vec<_>>())
        })
        .unwrap_or_default();

    if summary.snapshots.is_empty() && file_changes.is_empty() && operation_logs.is_empty() {
        return "<think>\n我已经检查了今天的前台活动快照、文件变更和操作意图日志，但当前数据库里还没有可用于总结的记录。\n</think>\n\n今天暂时还没有监听到可汇总的本地操作数据。你可以先继续使用电脑一段时间，再让我帮你总结。".to_string();
    }

    let mut lines = vec![
        "<think>".to_string(),
        "我先读取今天的本地监听数据，包括前台窗口快照、文件变更记录和操作意图日志，再按活跃时长、切换次数、主要工作分类和最近动作整理成摘要。".to_string(),
        "</think>".to_string(),
        String::new(),
        "我已经根据本地监听数据整理了你今天的活动：".to_string(),
        format!(
            "活跃时长约 {}，空闲时长约 {}，上下文切换 {} 次。",
            format_minutes(summary.total_active_minutes),
            format_minutes(summary.total_idle_minutes),
            summary.context_switches,
        ),
    ];

    if !summary.categories.is_empty() {
        let category_text = summary
            .categories
            .iter()
            .take(4)
            .map(|item| format!("{} {} ", item.category, format_minutes(item.minutes as i64)))
            .collect::<String>()
            .trim()
            .replace("  ", "，");
        lines.push(format!("主要时间分布：{}。", category_text));
    }

    if let Some(session) = summary.sessions.first() {
        lines.push(format!(
            "最长连续工作段是 {}，时间在 {} - {}，持续约 {}。",
            session.label,
            session.start_time,
            session.end_time,
            format_minutes(session.minutes),
        ));
    }

    if !summary.snapshots.is_empty() {
        lines.push(String::new());
        lines.push("最近前台活动：".to_string());
        for snapshot in summary.snapshots.iter().take(5) {
            let activity = if !snapshot.details.trim().is_empty() {
                snapshot.details.clone()
            } else if snapshot.window_title.is_empty() {
                snapshot.app_name.clone()
            } else {
                format!("{} / {}", snapshot.app_name, snapshot.window_title)
            };
            let time = snapshot.timestamp.get(11..16).unwrap_or(&snapshot.timestamp);
            lines.push(format!("- {} {} [{}]", time, activity, snapshot.category));
        }
    }

    if !file_changes.is_empty() {
        lines.push(String::new());
        lines.push("最近文件变更：".to_string());
        for (file_path, change_type, timestamp) in file_changes.iter().take(5) {
            let time = timestamp.get(11..16).unwrap_or(timestamp);
            lines.push(format!(
                "- {} {} {}",
                time,
                change_type,
                shorten_file_path(file_path),
            ));
        }
    }

    if !operation_logs.is_empty() {
        lines.push(String::new());
        lines.push("监听判断出的工作意图：".to_string());
        for (intention, file_path, timestamp) in operation_logs.iter().take(3) {
            let time = timestamp.get(11..16).unwrap_or(timestamp);
            lines.push(format!(
                "- {} {} [{}]",
                time,
                intention,
                shorten_file_path(file_path),
            ));
        }
    }

    lines.join("\n")
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
    db: State<'_, DbPool>,
    watcher_generation: State<'_, FileWatcherGeneration>,
) -> Result<(), String> {
    let app_data_dir: PathBuf = app.path().app_data_dir().map_err(|e| e.to_string())?;

    crate::settings::save_settings_to_disk(&app_data_dir, &new_settings)?;
    crate::settings::save_settings_to_home_dir(&new_settings)?;

    let mut s = settings.lock().unwrap();
    *s = new_settings;

    let effective_watch_paths: Vec<PathBuf> = if s.watch_paths.is_empty() {
        crate::settings::auto_detect_watch_paths()
            .into_iter()
            .map(PathBuf::from)
            .collect()
    } else {
        s.watch_paths.iter().map(PathBuf::from).collect()
    };

    stop_file_watcher(watcher_generation.inner());
    if !effective_watch_paths.is_empty() {
        start_file_watcher(
            db.inner().clone(),
            effective_watch_paths,
            watcher_generation.inner().clone(),
        );
    }
    Ok(())
}

/// 保存设置到用户主目录 ~/.autoheart
#[tauri::command]
pub fn save_settings_to_home(
    new_settings: AppSettings,
    settings: State<'_, SettingsHandle>,
) -> Result<(), String> {
    crate::settings::save_settings_to_home_dir(&new_settings)?;
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

fn trim_text(value: &str, max_chars: usize) -> String {
    let trimmed = value.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    let shortened = trimmed.chars().take(max_chars).collect::<String>();
    format!("{}...", shortened)
}

fn json_array_string(values: Vec<String>) -> String {
    serde_json::to_string(&values).unwrap_or_else(|_| "[]".to_string())
}

fn update_conversation_memory(db: &DbPool, conversation: &crate::conversation::Conversation) {
    let last_user = conversation
        .messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
        .map(|message| trim_text(&message.content, 120))
        .unwrap_or_default();
    let last_assistant = conversation
        .messages
        .iter()
        .rev()
        .find(|message| message.role == "assistant")
        .map(|message| trim_text(&message.content, 180))
        .unwrap_or_default();
    let summary = if last_assistant.is_empty() {
        format!("最近围绕“{}”继续讨论。", last_user)
    } else {
        format!("最近围绕“{}”进行了讨论，最新回答聚焦于：{}", last_user, last_assistant)
    };
    let open_questions = if last_user.is_empty() {
        vec![]
    } else {
        vec![last_user.clone()]
    };
    let next_steps = if last_assistant.is_empty() {
        vec![]
    } else {
        vec![last_assistant.clone()]
    };

    if let Ok(db) = db.lock() {
        let _ = db.execute(
            "INSERT INTO conversation_memory
             (conversation_id, summary, current_goal, decisions, open_questions, next_steps, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))
             ON CONFLICT(conversation_id) DO UPDATE SET
                summary = excluded.summary,
                current_goal = excluded.current_goal,
                decisions = excluded.decisions,
                open_questions = excluded.open_questions,
                next_steps = excluded.next_steps,
                updated_at = datetime('now')",
            rusqlite::params![
                conversation.id,
                summary,
                last_user,
                "[]",
                json_array_string(open_questions),
                json_array_string(next_steps)
            ],
        );
    }
}

fn update_project_memory(db: &DbPool) {
    let Ok(db) = db.lock() else {
        return;
    };

    let product_goal = "打造一个能感知工作上下文、形成工作片段、并在重启后延续项目记忆的桌面助手。".to_string();
    let current_focus = db
        .query_row(
            "SELECT current_goal
             FROM conversation_memory
             WHERE current_goal != ''
             ORDER BY updated_at DESC
             LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_else(|_| "继续完善 Auto-Heart 的上下文追踪与记忆能力。".to_string());

    let recent_summaries = db
        .prepare(
            "SELECT summary
             FROM work_sessions
             WHERE date(start_time) = date('now')
             ORDER BY updated_at DESC
             LIMIT 3",
        )
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map([], |row| row.get::<_, String>(0))
                .ok()
                .map(|rows| rows.filter_map(|row| row.ok()).collect::<Vec<_>>())
        })
        .unwrap_or_default();
    let pain_points = vec!["心跳信息需要更贴近用户真实工作片段".to_string()];
    let decisions = vec![
        "浏览器与编辑器场景优先追求任务级/意图级总结，不追求点击级监控".to_string(),
        "对话上下文需要跨重启自动恢复".to_string(),
        "工作片段、文件上下文和长期记忆都要持久化到本地数据库".to_string(),
    ];
    let constraints = vec![
        "尽量基于桌面侧与本地数据完成感知，避免一开始就引入过重插件依赖".to_string(),
    ];

    let _ = db.execute(
        "INSERT INTO project_memory
         (project_key, product_goal, current_focus, confirmed_decisions, constraints, user_preferences, known_pain_points, milestones, updated_at)
         VALUES ('auto-heart', ?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'))
         ON CONFLICT(project_key) DO UPDATE SET
            product_goal = excluded.product_goal,
            current_focus = excluded.current_focus,
            confirmed_decisions = excluded.confirmed_decisions,
            constraints = excluded.constraints,
            user_preferences = excluded.user_preferences,
            known_pain_points = excluded.known_pain_points,
            milestones = excluded.milestones,
            updated_at = datetime('now')",
        rusqlite::params![
            product_goal,
            current_focus,
            json_array_string(decisions),
            json_array_string(constraints),
            json_array_string(vec![
                "希望心跳跟随用户操作，而不是简单定时采样".to_string(),
                "希望系统能像 Codex 一样在下次进入时延续上下文".to_string(),
            ]),
            json_array_string(pain_points),
            json_array_string(recent_summaries),
        ],
    );
}

fn build_memory_context(db: &DbPool, session_id: &str) -> String {
    let Ok(db) = db.lock() else {
        return String::new();
    };

    let project_memory = db
        .query_row(
            "SELECT product_goal, current_focus, confirmed_decisions, constraints, user_preferences, known_pain_points
             FROM project_memory
             WHERE project_key = 'auto-heart'
             LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .ok();

    let conversation_memory = db
        .query_row(
            "SELECT summary, current_goal, open_questions, next_steps
             FROM conversation_memory
             WHERE conversation_id = ?1
             LIMIT 1",
            rusqlite::params![session_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .ok();

    let recent_sessions = db
        .prepare(
            "SELECT summary, start_time, end_time
             FROM work_sessions
             WHERE end_time > datetime('now', '-12 hours')
             ORDER BY end_time DESC
             LIMIT 3",
        )
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map([], |row| {
                Ok(format!(
                    "{} ({} - {})",
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?
                ))
            })
            .ok()
            .map(|rows| rows.filter_map(|row| row.ok()).collect::<Vec<_>>())
        })
        .unwrap_or_default();

    let recent_files = db
        .prepare(
            "SELECT file_path, latest_summary
             FROM file_contexts
             ORDER BY last_changed_at DESC
             LIMIT 5",
        )
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map([], |row| {
                Ok(format!(
                    "{} => {}",
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?
                ))
            })
            .ok()
            .map(|rows| rows.filter_map(|row| row.ok()).collect::<Vec<_>>())
        })
        .unwrap_or_default();

    let mut sections = Vec::new();
    if let Some((goal, focus, decisions, constraints, preferences, pain_points)) = project_memory {
        sections.push(format!(
            "项目长期记忆\n- 产品目标：{}\n- 当前重点：{}\n- 已确认决策：{}\n- 约束：{}\n- 用户偏好：{}\n- 当前痛点：{}",
            goal, focus, decisions, constraints, preferences, pain_points
        ));
    }
    if let Some((summary, current_goal, open_questions, next_steps)) = conversation_memory {
        sections.push(format!(
            "当前会话记忆\n- 摘要：{}\n- 当前目标：{}\n- 未决问题：{}\n- 下一步：{}",
            summary, current_goal, open_questions, next_steps
        ));
    }
    if !recent_sessions.is_empty() {
        sections.push(format!("最近工作片段\n- {}", recent_sessions.join("\n- ")));
    }
    if !recent_files.is_empty() {
        sections.push(format!("最近文件上下文\n- {}", recent_files.join("\n- ")));
    }

    sections.join("\n\n")
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
pub fn delete_conversation(id: String, app: AppHandle, db: State<'_, DbPool>) -> Result<(), String> {
    let data_dir = app.path().app_data_dir().unwrap_or_default();
    crate::conversation::delete_conversation(&data_dir, &id)?;
    if let Ok(db) = db.lock() {
        let _ = db.execute(
            "DELETE FROM conversation_memory WHERE conversation_id = ?1",
            rusqlite::params![id],
        );
    }
    Ok(())
}

#[tauri::command]
pub fn add_today_task(
    task: String,
    time: Option<String>,
    tag: Option<String>,
    db: State<'_, DbPool>,
) -> Result<(), String> {
    let db = db.lock().map_err(|error| error.to_string())?;
    let mut tasks = if let Some((row_id, raw_text, parsed_tasks)) = load_latest_today_task_row(&db) {
        let mut tasks: Vec<TodayTask> = serde_json::from_str(&parsed_tasks).unwrap_or_default();
        tasks.push(TodayTask {
            time: time.unwrap_or_default(),
            task: task.trim().to_string(),
            tag: tag.unwrap_or_default(),
            status: "pending".to_string(),
        });
        db.execute(
            "UPDATE intent_history
             SET parsed_tasks = ?1,
                 completion_status = 'active',
                 raw_text = ?2
             WHERE id = ?3",
            rusqlite::params![
                serde_json::to_string(&tasks).map_err(|error| error.to_string())?,
                if raw_text.trim().is_empty() { "手动录入今日任务" } else { raw_text.trim() },
                row_id
            ],
        )
        .map_err(|error| error.to_string())?;
        return Ok(());
    } else {
        Vec::new()
    };

    tasks.push(TodayTask {
        time: time.unwrap_or_default(),
        task: task.trim().to_string(),
        tag: tag.unwrap_or_default(),
        status: "pending".to_string(),
    });

    db.execute(
        "INSERT INTO intent_history (id, raw_text, parsed_tasks, completion_status)
         VALUES (?1, ?2, ?3, 'active')",
        rusqlite::params![
            Uuid::new_v4().to_string(),
            "手动录入今日任务",
            serde_json::to_string(&tasks).map_err(|error| error.to_string())?
        ],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn update_today_task_status(
    index: usize,
    status: String,
    db: State<'_, DbPool>,
) -> Result<(), String> {
    if !matches!(status.as_str(), "pending" | "active" | "done") {
        return Err("invalid status".to_string());
    }

    let db = db.lock().map_err(|error| error.to_string())?;
    let Some((_, _, parsed_tasks)) = load_latest_today_task_row(&db) else {
        return Err("today tasks not found".to_string());
    };
    let mut tasks: Vec<TodayTask> = serde_json::from_str(&parsed_tasks).unwrap_or_default();
    let Some(task) = tasks.get_mut(index) else {
        return Err("task index out of range".to_string());
    };
    task.status = status;

    update_latest_today_tasks(&db, &tasks)
}

#[tauri::command]
pub fn update_today_task(
    index: usize,
    task: String,
    time: Option<String>,
    tag: Option<String>,
    db: State<'_, DbPool>,
) -> Result<(), String> {
    let db = db.lock().map_err(|error| error.to_string())?;
    let Some((_, _, parsed_tasks)) = load_latest_today_task_row(&db) else {
        return Err("today tasks not found".to_string());
    };
    let mut tasks: Vec<TodayTask> = serde_json::from_str(&parsed_tasks).unwrap_or_default();
    let Some(current) = tasks.get_mut(index) else {
        return Err("task index out of range".to_string());
    };

    current.task = task.trim().to_string();
    current.time = time.unwrap_or_default();
    current.tag = tag.unwrap_or_default();

    update_latest_today_tasks(&db, &tasks)
}

#[tauri::command]
pub fn delete_today_task(index: usize, db: State<'_, DbPool>) -> Result<(), String> {
    let db = db.lock().map_err(|error| error.to_string())?;
    let Some((_, _, parsed_tasks)) = load_latest_today_task_row(&db) else {
        return Err("today tasks not found".to_string());
    };
    let mut tasks: Vec<TodayTask> = serde_json::from_str(&parsed_tasks).unwrap_or_default();
    if index >= tasks.len() {
        return Err("task index out of range".to_string());
    }
    tasks.remove(index);

    update_latest_today_tasks(&db, &tasks)
}

#[tauri::command]
pub fn move_today_task(index: usize, direction: String, db: State<'_, DbPool>) -> Result<(), String> {
    let db = db.lock().map_err(|error| error.to_string())?;
    let Some((_, _, parsed_tasks)) = load_latest_today_task_row(&db) else {
        return Err("today tasks not found".to_string());
    };
    let mut tasks: Vec<TodayTask> = serde_json::from_str(&parsed_tasks).unwrap_or_default();
    if index >= tasks.len() {
        return Err("task index out of range".to_string());
    }

    let target = match direction.as_str() {
        "up" if index > 0 => index - 1,
        "down" if index + 1 < tasks.len() => index + 1,
        "up" | "down" => return Ok(()),
        _ => return Err("invalid direction".to_string()),
    };

    tasks.swap(index, target);
    update_latest_today_tasks(&db, &tasks)
}

#[tauri::command]
pub fn parse_today_intent_now(
    app: AppHandle,
    settings: State<'_, SettingsHandle>,
    db: State<'_, DbPool>,
) -> Result<bool, String> {
    let settings_snapshot = settings.lock().map_err(|error| error.to_string())?.clone();
    Ok(crate::heartbeat::parse_pending_intents_now(&db, &app, &settings_snapshot))
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
    let mut conv = crate::conversation::get_conversation(&data_dir, &session_id)
        .unwrap_or_else(|| crate::conversation::Conversation::new(&content));

    let user_msg = crate::conversation::Message {
        id: Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: content.clone(),
        timestamp: chrono::Local::now().to_rfc3339(),
    };
    conv.messages.push(user_msg);
    conv.updated_at = chrono::Local::now().to_rfc3339();
    crate::conversation::save_conversation(&data_dir, &conv)?;

    let settings_snap = settings.lock().unwrap().clone();

    if should_use_local_activity_summary(&content) {
        let local_reply = build_local_report_from_data(&content, &db, &settings_snap)
            .unwrap_or_else(|| build_local_activity_chat_reply(&db));
        let assistant_msg = crate::conversation::Message {
            id: Uuid::new_v4().to_string(),
            role: "assistant".to_string(),
            content: local_reply,
            timestamp: chrono::Local::now().to_rfc3339(),
        };

        conv.messages.push(assistant_msg.clone());
        conv.updated_at = chrono::Local::now().to_rfc3339();
        crate::conversation::save_conversation(&data_dir, &conv)?;
        update_conversation_memory(&db, &conv);
        update_project_memory(&db);

        return Ok(ChatMessage {
            id: assistant_msg.id,
            role: assistant_msg.role,
            content: assistant_msg.content,
            timestamp: assistant_msg.timestamp,
        });
    }

    let memory_context = build_memory_context(&db, &conv.id);
    let mut oai_messages: Vec<crate::model_router::OaiMessage> = vec![];
    if !memory_context.is_empty() {
        oai_messages.push(crate::model_router::OaiMessage {
            role: "system".to_string(),
            content: format!(
                "你是 Auto-Heart 的长期协作助手。以下是持久化上下文，请把它当作高优先级项目记忆，在回答时保持连续性，但不要机械复述。\n\n{}",
                memory_context
            ),
        });
    }
    oai_messages.extend(conv.messages.iter().map(|message| crate::model_router::OaiMessage {
        role: message.role.clone(),
        content: message.content.clone(),
    }));

    let chat_model = if settings_snap.chat_model.is_empty() {
        &settings_snap.middle_model
    } else {
        &settings_snap.chat_model
    };
    let chat_model_name = if settings_snap.chat_model_name.is_empty() {
        &settings_snap.middle_model_name
    } else {
        &settings_snap.chat_model_name
    };
    let model_config = crate::model_router::build_model_config(
        chat_model,
        chat_model_name,
        &settings_snap,
    )
    .ok_or("Chat model not configured. Please set chat_model in settings.")?;

    let response =
        crate::model_router::call_chat_model_with_messages(&model_config, &oai_messages).await?;

    let assistant_msg = crate::conversation::Message {
        id: Uuid::new_v4().to_string(),
        role: "assistant".to_string(),
        content: response,
        timestamp: chrono::Local::now().to_rfc3339(),
    };

    conv.messages.push(assistant_msg.clone());
    conv.updated_at = chrono::Local::now().to_rfc3339();
    crate::conversation::save_conversation(&data_dir, &conv)?;
    update_conversation_memory(&db, &conv);

    if contains_intent_keywords(&content) {
        let _ = parse_intent_from_chat(&content, &settings_snap, &db);
    }
    update_project_memory(&db);

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

    let handle = tokio::runtime::Handle::current();
    let router = crate::model_router::ModelRouter::new();
    let response = handle.block_on(router.call_with_config(
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
