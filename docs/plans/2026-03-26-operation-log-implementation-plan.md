# 操作日志与智能体实现计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 为 Auto-Heart 添加操作日志收集（5分钟心跳 + LLM意图分析）、对话Tab、智能体主动建议功能。

**Architecture:**
- 新增 `operation_log` 表存储 LLM 意图分析结果
- 新增 operation_log 心跳（5分钟），调用中层模型分析文件变更
- 新增对话 Tab，使用深层模型回答日志查询
- 主动建议引擎：定时（30分钟）+ 事件触发（关键模块变更）
- 关键模块变更通过 Orb 气泡展示

**Tech Stack:** Tauri 2.x, React, SQLite, reqwest (LLM HTTP calls), notify (file watching)

---

## 阶段一：数据库 + 配置

### Task 1: 添加 operation_log 表到 database.rs

**Files:**
- Modify: `auto-heart/src-tauri/src/database.rs:30-111`

**Step 1: 添加 operation_log 建表 SQL**

在 `create_tables()` 函数的 `conn.execute_batch()` 中，在 `file_changes` 表定义之后添加：

```rust
// 操作日志（LLM 意图分析结果）
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

CREATE INDEX IF NOT EXISTS idx_operation_log_time ON operation_log(timestamp);
CREATE INDEX IF NOT EXISTS idx_operation_log_chunk ON operation_log(chunk_id);
```

**Step 2: 运行测试验证**

```bash
cd D:/Agent/Auto-Heart/auto-heart/src-tauri && cargo build 2>&1
```

预期：编译成功

**Step 3: Commit**

```bash
git add auto-heart/src-tauri/src/database.rs
git commit -m "feat(db): add operation_log table for intention analysis"
```

---

### Task 2: AppSettings 添加新配置字段

**Files:**
- Modify: `auto-heart/src-tauri/src/settings.rs`

**Step 1: 添加新字段到 AppSettings struct**

在 `AppSettings` struct 中添加：

```rust
// 主动建议
pub proactive_suggestions: bool,
pub critical_keywords: String,
```

**Step 2: Default 实现中添加默认值**

```rust
proactive_suggestions: true,
critical_keywords: "auth,security,password,token,payment,config,middleware,permission".to_string(),
```

**Step 3: Build 验证**

```bash
cd D:/Agent/Auto-Heart/auto-heart/src-tauri && cargo build 2>&1
```

**Step 4: Commit**

```bash
git add auto-heart/src-tauri/src/settings.rs
git commit -m "feat(settings): add proactive_suggestions and critical_keywords config"
```

---

## 阶段二：心跳 - operation_log 收集

### Task 3: 实现意图分析函数

**Files:**
- Modify: `auto-heart/src-tauri/src/heartbeat.rs`

**Step 1: 添加数据结构**

在文件顶部添加：

```rust
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
```

**Step 2: 添加 `collect_recent_file_changes` 函数**

```rust
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
```

**Step 3: 添加 `call_intention_analysis` 函数**

```rust
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
        .build()?;
    let router = ModelRouter::new();
    let response = rt.block_on(router.call_with_config(
        &config,
        &prompt,
        Some("你是 Auto-Heart 的意图分析助手，只输出 JSON。"),
    )).ok()?;

    // 解析 JSON
    let json_str = extract_json(&response);
    serde_json::from_str(json_str).ok()
}
```

**Step 4: 添加 `save_operation_logs` 函数**

```rust
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
```

**Step 5: Build 验证**

```bash
cd D:/Agent/Auto-Heart/auto-heart/src-tauri && cargo build 2>&1
```

预期：编译成功（可能有 unused warnings）

**Step 6: Commit**

```bash
git add auto-heart/src-tauri/src/heartbeat.rs
git commit -m "feat(heartbeat): add intention analysis functions for operation_log"
```

---

### Task 4: 实现 operation_log 心跳（5分钟）

**Files:**
- Modify: `auto-heart/src-tauri/src/heartbeat.rs`
- Modify: `auto-heart/src-tauri/src/lib.rs:134-137`

**Step 1: 添加心跳启动函数**

在 heartbeat.rs 末尾添加：

```rust
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
                save_operation_logs(&db, &analysis, &chunk_id);
            }
        }
    });
}
```

**Step 2: 在 lib.rs setup 中注册心跳**

在 `start_shallow_heartbeat` 之后添加：

```rust
start_operation_log_heartbeat(
    app.handle().clone(),
    db.clone(),
    settings_handle.clone(),
);
```

**Step 3: Build 验证**

```bash
cd D:/Agent/Auto-Heart/auto-heart/src-tauri && cargo build 2>&1
```

预期：编译成功

**Step 4: Commit**

```bash
git add auto-heart/src-tauri/src/heartbeat.rs auto-heart/src-tauri/src/lib.rs
git commit -m "feat(heartbeat): add 5min operation_log heartbeat"
```

---

## 阶段三：对话功能

### Task 5: 添加 Rust 命令接口

**Files:**
- Modify: `auto-heart/src-tauri/src/commands.rs`

**Step 1: 添加数据结构**

```rust
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
```

**Step 2: 添加 query_operation_logs 命令**

```rust
#[tauri::command]
fn query_operation_logs(
    date: String,  // "2026-03-26" 或 "today"
    db: State<'_, DbPool>,
) -> Vec<OperationLogEntry> {
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
```

**Step 3: 添加 search_file_changes 命令**

```rust
#[tauri::command]
fn search_file_changes(
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
```

**Step 4: 添加 get_trend_stats 命令**

```rust
#[tauri::command]
fn get_trend_stats(
    days: i32,
    db: State<'_, DbPool>,
) -> TrendStats {
    let db = match db.lock() {
        Ok(d) => d,
        Err(_) => return TrendStats { days, total_changes: 0, avg_per_day: 0.0, top_modules: vec![] },
    };

    let param = format!("-{} days", days);
    let total: i32 = db.query_row(
        "SELECT COUNT(*) FROM file_changes WHERE timestamp > datetime('now', ?1)",
        rusqlite::params![param],
    ).unwrap_or(0);

    let avg = total as f64 / days as f64;

    // 统计高频模块
    let mut stmt = match db.prepare(
        "SELECT file_path, COUNT(*) as cnt FROM file_changes \
         WHERE timestamp > datetime('now', ?1) \
         GROUP BY file_path ORDER BY cnt DESC LIMIT 5",
    ) {
        Ok(s) => s,
        Err(_) => return TrendStats { days, total_changes: total, avg_per_day: avg, top_modules: vec![] },
    };

    let top: Vec<String> = stmt.query_map(rusqlite::params![param], |row| {
        row.get::<_, String>(0)
    })
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default();

    TrendStats {
        days,
        total_changes: total,
        avg_per_day: avg,
        top_modules: top,
    }
}
```

**Step 5: 在 lib.rs invoke_handler 中注册新命令**

```rust
// 对话 + 日志查询
commands::query_operation_logs,
commands::search_file_changes,
commands::get_trend_stats,
```

**Step 6: Build 验证**

```bash
cd D:/Agent/Auto-Heart/auto-heart/src-tauri && cargo build 2>&1
```

**Step 7: Commit**

```bash
git add auto-heart/src-tauri/src/commands.rs auto-heart/src-tauri/src/lib.rs
git commit -m "feat(commands): add log query, file search, trend stats commands"
```

---

### Task 6: 对话 Tab 前端组件

**Files:**
- Create: `auto-heart/src/pages/ConversationTab.tsx`

**Step 1: 创建 ConversationTab.tsx**

```tsx
import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface ChatMessage {
  role: 'user' | 'assistant';
  content: string;
  timestamp: string;
}

interface OperationLogEntry {
  id: string;
  timestamp: string;
  file_path: string;
  intention_desc: string;
  tags: string[];
}

export default function ConversationTab() {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState('');
  const [loading, setLoading] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  const scrollToBottom = () => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  };

  useEffect(() => {
    scrollToBottom();
  }, [messages]);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim() || loading) return;

    const userMsg: ChatMessage = {
      role: 'user',
      content: input.trim(),
      timestamp: new Date().toISOString(),
    };
    setMessages(prev => [...prev, userMsg]);
    setInput('');
    setLoading(true);

    try {
      // 路由判断
      const lower = userMsg.content.toLowerCase();
      let response: string;

      if (lower.includes('今天') && (lower.includes('做了') || lower.includes('做了什么'))) {
        // 查询今日日志
        const logs = await invoke<OperationLogEntry[]>('query_operation_logs', { date: 'today' });
        if (logs.length === 0) {
          response = '今天暂无记录的操作。可能还没有文件变更，或变更还未被分析。';
        } else {
          const summary = logs.slice(0, 10).map(l =>
            `- ${l.intention_desc} (${l.file_path})`
          ).join('\n');
          response = `今天共记录了 ${logs.length} 项操作：\n${summary}`;
        }
      } else if (lower.includes('找') && lower.includes('文件')) {
        const keyword = lower.replace(/.*找一下|文件/g, '').trim() || '%';
        const files = await invoke<[string, string, string][]>('search_file_changes', {
          keyword,
          daysBack: 7,
        });
        if (files.length === 0) {
          response = `最近7天没有找到包含"${keyword}"的文件变更。`;
        } else {
          const list = files.slice(0, 10).map(([path, type, time]) =>
            `- ${path} [${type}] @ ${time}`
          ).join('\n');
          response = `最近7天找到 ${files.length} 个相关文件：\n${list}`;
        }
      } else if (lower.includes('周') && (lower.includes('平均') || lower.includes('多少'))) {
        const stats = await invoke<{ avg_per_day: number; total_changes: number; top_modules: string[] }>('get_trend_stats', { days: 7 });
        response = `最近7天统计：\n- 总变更：${stats.total_changes} 次\n- 日均：${stats.avg_per_day.toFixed(1)} 次\n- 高频模块：${stats.top_modules.join(', ') || '无'}。`;
      } else {
        response = '我目前支持：\n- "今天我做了什么？" - 查询今日操作日志\n- "找一下 XX 文件" - 搜索文件变更\n- "这周平均多少" - 趋势统计\n\n请告诉我你想查询什么？';
      }

      setMessages(prev => [...prev, {
        role: 'assistant',
        content: response,
        timestamp: new Date().toISOString(),
      }]);
    } catch (err) {
      setMessages(prev => [...prev, {
        role: 'assistant',
        content: `抱歉，查询失败：${String(err)}`,
        timestamp: new Date().toISOString(),
      }]);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%' }}>
      {/* 消息列表 */}
      <div style={{ flex: 1, overflow: 'auto', padding: 16, display: 'flex', flexDirection: 'column', gap: 12 }}>
        {messages.length === 0 && (
          <div style={{ textAlign: 'center', color: 'var(--color-text-tertiary)', fontSize: 12, marginTop: 40 }}>
            问我关于今天的操作日志吧<br />
            <span style={{ fontSize: 11 }}>例如："今天我做了什么？"</span>
          </div>
        )}
        {messages.map((msg, i) => (
          <div key={i} style={{
            display: 'flex',
            flexDirection: 'column',
            alignItems: msg.role === 'user' ? 'flex-end' : 'flex-start',
          }}>
            <div style={{
              maxWidth: '80%',
              padding: '10px 14px',
              borderRadius: 12,
              background: msg.role === 'user'
                ? 'var(--color-brand-light)'
                : 'var(--color-background-secondary)',
              color: msg.role === 'user'
                ? 'var(--color-brand)'
                : 'var(--color-text-primary)',
              fontSize: 13,
              lineHeight: 1.5,
              whiteSpace: 'pre-wrap',
            }}>
              {msg.content}
            </div>
          </div>
        ))}
        {loading && (
          <div style={{ color: 'var(--color-text-tertiary)', fontSize: 12 }}>思考中...</div>
        )}
        <div ref={messagesEndRef} />
      </div>

      {/* 输入框 */}
      <form onSubmit={handleSubmit} style={{
        padding: '12px 16px',
        borderTop: '0.5px solid var(--color-border-tertiary)',
        display: 'flex',
        gap: 8,
      }}>
        <input
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="问我关于今天的操作..."
          style={{
            flex: 1,
            padding: '8px 12px',
            fontSize: 13,
            background: 'var(--color-background-tertiary)',
            border: '0.5px solid var(--color-border-primary)',
            borderRadius: 8,
            color: 'var(--color-text-primary)',
            outline: 'none',
          }}
        />
        <button
          type="submit"
          disabled={loading || !input.trim()}
          style={{
            padding: '8px 16px',
            fontSize: 13,
            borderRadius: 8,
            background: loading ? 'var(--color-background-secondary)' : 'var(--color-brand)',
            border: 'none',
            color: '#fff',
            cursor: loading ? 'default' : 'pointer',
          }}
        >
          发送
        </button>
      </form>
    </div>
  );
}
```

**Step 2: Commit**

```bash
git add auto-heart/src/pages/ConversationTab.tsx
git commit -m "feat(frontend): add ConversationTab with log query UI"
```

---

### Task 7: 注册对话 Tab

**Files:**
- Modify: `auto-heart/src/pages/MainWindow.tsx`

**Step 1: 添加对话 Tab 类型**

```tsx
type Tab = 'today' | 'semantic-map' | 'settings' | 'conversation';  // 新增 conversation
```

**Step 2: 在 tabs 数组中添加**

```tsx
const tabs: { id: Tab; label: string }[] = [
  { id: 'today', label: '今天' },
  { id: 'semantic-map', label: '语义地图' },
  { id: 'conversation', label: '对话' },
  { id: 'settings', label: '设置' },
];
```

**Step 3: 添加 ConversationTab import**

```tsx
import ConversationTab from './ConversationTab';
```

**Step 4: 添加渲染分支**

```tsx
{activeTab === 'conversation' && <ConversationTab />}
```

**Step 5: Build 验证**

```bash
cd D:/Agent/Auto-Heart/auto-heart && npx tsc --noEmit 2>&1
```

**Step 6: Commit**

```bash
git add auto-heart/src/pages/MainWindow.tsx
git commit -m "feat(frontend): register ConversationTab in MainWindow"
```

---

## 阶段四：主动建议 + 气泡提醒

### Task 8: 实现主动建议引擎

**Files:**
- Modify: `auto-heart/src-tauri/src/heartbeat.rs`

**Step 1: 添加关键模块检测函数**

在 heartbeat.rs 添加：

```rust
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

fn get_critical_keywords(settings: &crate::settings::AppSettings) -> Vec<String> {
    settings
        .critical_keywords
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}
```

**Step 2: 添加 `emit_agent_alert` 函数**

```rust
fn emit_agent_alert(app: &AppHandle, alert_type: &str, title: &str, message: &str) {
    let _ = app.emit(
        "agent:alert",
        serde_json::json!({
            "type": alert_type,
            "title": title,
            "message": message,
        }),
    );
}
```

**Step 3: 添加主动建议检查函数**

```rust
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
        return; // 同时只触发一种提醒
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
            )
            .unwrap_or(0);
        count > 0
    };

    if !intent_updated {
        // 检查是否有配置路径
        if !settings.intent_doc_path.is_empty() {
            emit_agent_alert(
                app,
                "intent_reminder",
                "今日意图待更新",
                "你今天还没有更新意图文档，要看一下吗？",
            );
        }
    }
}
```

**Step 4: 修改 operation_log 心跳，集成主动建议**

在 `start_operation_log_heartbeat` 循环中添加检查：

```rust
// 在意图分析之后添加
check_proactive_suggestions(&app, &db, &settings_snap, &changes);
```

**Step 5: Build 验证**

```bash
cd D:/Agent/Auto-Heart/auto-heart/src-tauri && cargo build 2>&1
```

**Step 6: Commit**

```bash
git add auto-heart/src-tauri/src/heartbeat.rs
git commit -m "feat(heartbeat): add proactive suggestions engine"
```

---

### Task 9: Orb 气泡支持 agent 提醒

**Files:**
- Modify: `auto-heart/src/App.tsx`
- Modify: `auto-heart/src/components/SpeechBubble.tsx` (如需复用)

**Step 1: 在 App.tsx 中监听 agent:alert 事件**

```tsx
useEffect(() => {
  let unlistenAlert: (() => void) | undefined;

  const setup = async () => {
    if (!isTauriRuntime()) return;
    const { listen } = await import('@tauri-apps/api/event');

    unlistenAlert = await listen<{
      type: 'critical' | 'intent_reminder' | 'queue_warning';
      title: string;
      message: string;
    }>('agent:alert', (event) => {
      setBubbleMessage({
        id: `agent-${Date.now()}`,
        title: event.payload.title,
        content: event.payload.message,
        type: 'agent',
      });
      setOrbState('speaking');
    });
  };

  setup();
  return () => { unlistenAlert?.(); };
}, []);
```

**Step 2: 修改 bubbleMessage 类型支持 agent**

```tsx
const [bubbleMessage, setBubbleMessage] = useState<{
  id: string;
  title: string;
  content: string;
  type: 'message' | 'report' | 'agent';
} | null>(null);
```

**Step 3: 修改 handleBubbleDismiss 和 handleBubbleAction**

```tsx
const handleBubbleDismiss = async () => {
  setBubbleMessage(null);
  setOrbState('idle');
};

const handleBubbleAction = async () => {
  if (bubbleMessage?.type === 'agent') {
    // agent 提醒点击"查看"打开主窗口
    try { await invoke('open_main_window'); } catch {}
  }
  setBubbleMessage(null);
  setOrbState('idle');
};
```

**Step 4: Build 验证**

```bash
cd D:/Agent/Auto-Heart/auto-heart && npx tsc --noEmit 2>&1
```

**Step 5: Commit**

```bash
git add auto-heart/src/App.tsx
git commit -m "feat(frontend): support agent alerts via Orb bubble"
```

---

## 验证步骤

### 完整验证流程

1. **启动应用**
   ```bash
   npm run tauri dev
   ```

2. **触发文件变更，等待 5 分钟**
   - 观察日志输出 `[operation_log] 分析了 X 项变更`
   - 检查数据库 `operation_log` 表是否有记录

3. **测试对话 Tab**
   - 问："今天我做了什么？"
   - 应返回今日操作日志摘要

4. **测试关键模块提醒**
   - 修改 `auth/guard.ts` 文件
   - 等待下一次心跳（5分钟内）
   - 观察 Orb 是否出现气泡提醒

5. **测试对话命令**
   - "找一下 auth 文件"
   - "这周平均多少"

---

## 风险与注意事项

1. **首次运行需要配置模型**：中层模型需要配置 API Key 才能进行意图分析
2. **Token 消耗**：5分钟一次 LLM 调用，注意监控用量
3. **数据库迁移**：新增表不影响现有数据
4. **气泡防抖**：同一提醒 15 分钟内不重复显示（前端状态控制）

