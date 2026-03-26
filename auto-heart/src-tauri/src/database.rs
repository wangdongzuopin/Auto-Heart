use rusqlite::{Connection, Result};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Auto-Heart 本地数据库
///
/// 语义地图完整 schema：
/// - semantic_modules: 模块理解记录
/// - decision_log: 决策日志
/// - tech_debt: 技术债记录
/// - intent_history: 意图历史
/// - message_queue: 消息队列
/// - file_changes: 文件变更记录（浅层心跳写入）

pub type DbPool = Arc<Mutex<Connection>>;

/// 初始化数据库，返回线程安全的连接池
pub fn init_database(app_data_dir: PathBuf) -> Result<DbPool> {
    let db_path = app_data_dir.join("auto_heart.db");
    let conn = Connection::open(db_path)?;

    // 启用 WAL 模式提升并发性能
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;

    create_tables(&conn)?;

    Ok(Arc::new(Mutex::new(conn)))
}

fn create_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        -- 模块理解记录（语义地图核心）
        CREATE TABLE IF NOT EXISTS semantic_modules (
            id          TEXT PRIMARY KEY,
            module_name TEXT NOT NULL UNIQUE,
            description TEXT NOT NULL DEFAULT '',
            dependencies TEXT DEFAULT '[]',
            recent_changes TEXT DEFAULT '',
            design_intent TEXT DEFAULT '',
            understanding TEXT DEFAULT '',
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- 决策日志
        CREATE TABLE IF NOT EXISTS decision_log (
            id          TEXT PRIMARY KEY,
            description TEXT NOT NULL,
            reason      TEXT DEFAULT '',
            related_file TEXT DEFAULT '',
            context     TEXT DEFAULT '',
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- 技术债记录
        CREATE TABLE IF NOT EXISTS tech_debt (
            id          TEXT PRIMARY KEY,
            description TEXT NOT NULL,
            impact      TEXT DEFAULT '',
            introduced_at TEXT NOT NULL DEFAULT (datetime('now')),
            resolved_at TEXT,
            related_file TEXT DEFAULT ''
        );

        -- 意图历史
        CREATE TABLE IF NOT EXISTS intent_history (
            id          TEXT PRIMARY KEY,
            raw_text    TEXT NOT NULL,
            parsed_tasks TEXT DEFAULT '[]',  -- JSON 数组
            completion_status TEXT DEFAULT 'pending',
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- 消息队列（沉默判断用）
        CREATE TABLE IF NOT EXISTS message_queue (
            id          TEXT PRIMARY KEY,
            priority    INTEGER NOT NULL DEFAULT 1,  -- 0=立即, 1=等自然节点, 2=日报附带
            title       TEXT NOT NULL,
            content     TEXT NOT NULL,
            status      TEXT NOT NULL DEFAULT 'pending',  -- pending, sent, dismissed
            created_at  TEXT NOT NULL DEFAULT (datetime('now')),
            sent_at     TEXT
        );

        -- 文件变更记录（浅层心跳写入）
        CREATE TABLE IF NOT EXISTS file_changes (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            file_path   TEXT NOT NULL,
            change_type TEXT NOT NULL,  -- create, modify, delete, rename
            timestamp   TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- 操作日志（LLM 意图分析结果）
        CREATE TABLE IF NOT EXISTS operation_log (
            id              TEXT PRIMARY KEY,
            timestamp       TEXT NOT NULL DEFAULT (datetime('now')),
            file_path       TEXT NOT NULL,
            change_type     TEXT NOT NULL,
            intention_desc  TEXT NOT NULL DEFAULT '',
            confidence      REAL DEFAULT 0.5,
            tags            TEXT DEFAULT '[]',
            chunk_id        TEXT
        );

        -- 日报
        CREATE TABLE IF NOT EXISTS daily_reports (
            id          TEXT PRIMARY KEY,
            date        TEXT NOT NULL UNIQUE,
            content     TEXT NOT NULL,
            status      TEXT NOT NULL DEFAULT 'draft',  -- draft, confirmed, sent
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        -- 索引
        CREATE INDEX IF NOT EXISTS idx_file_changes_time ON file_changes(timestamp);
        CREATE INDEX IF NOT EXISTS idx_message_queue_status ON message_queue(status, priority);
        CREATE INDEX IF NOT EXISTS idx_intent_history_date ON intent_history(created_at);
        CREATE INDEX IF NOT EXISTS idx_operation_log_time ON operation_log(timestamp);
        CREATE INDEX IF NOT EXISTS idx_operation_log_chunk ON operation_log(chunk_id);
        ",
    )?;

    Ok(())
}
