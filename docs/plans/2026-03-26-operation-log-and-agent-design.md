# 操作日志与智能体设计

## Context

用户希望 Auto-Heart 能够：
1. 每 5 分钟收集一次操作记录，形成当天的操作日志
2. 使用 LLM 分析文件变更，生成意图描述
3. 在主窗口新增"对话"Tab，支持询问日志、搜索文件、分析趋势
4. 智能体具备主动建议能力，检测关键模块变更时气泡提醒

## 整体架构

```
文件变更（notify 监听）
    │
    ├── file_changes 表（原始变更记录）
    │
    ├── 5min 心跳 → LLM 意图分析 → operation_log 表（意图日志）
    │
    ├── 中层心跳（10min）→ 语义理解 → semantic_modules + message_queue
    │
    └── 主动建议引擎（30min 定时 + 事件触发）
              │
              └── 关键模块变更 → 气泡提醒（Orb 旁）
```

## 一、数据库变更

### 新增表：operation_log

```sql
CREATE TABLE operation_log (
    id          TEXT PRIMARY KEY,
    timestamp   TEXT NOT NULL DEFAULT (datetime('now')),
    file_path   TEXT NOT NULL,
    change_type TEXT NOT NULL,  -- create / modify / delete
    intention_desc TEXT NOT NULL DEFAULT '',  -- LLM 生成的意图描述
    confidence  REAL DEFAULT 0.5,
    tags        TEXT DEFAULT '[]',  -- JSON: ["feature", "bugfix", ...]
    chunk_id    TEXT  -- 关联的心跳批次 ID，用于批量查询
);

CREATE INDEX idx_operation_log_time ON operation_log(timestamp);
CREATE INDEX idx_operation_log_chunk ON operation_log(chunk_id);
```

### 字段说明

| 字段 | 类型 | 说明 |
|------|------|------|
| intention_desc | String | LLM 生成的操作意图，如"正在修复 auth 模块的 refreshToken 校验问题" |
| confidence | f32 | LLM 置信度 0~1，过低时不写入 |
| tags | JSON Array | 分类标签：feature, bugfix, refactor, docs, chore, security |
| chunk_id | String | 心跳批次 ID，5 分钟内的变更归到同一批次 |

### LLM Prompt 示例

```
以下是你监测到的文件变更：

1. auth/guard.ts [modify]
2. UserService.java [modify]

请分析这些变更的意图，输出 JSON：

{
  "intentions": [
    {
      "file": "auth/guard.ts",
      "change_type": "modify",
      "description": "在 verify() 函数中添加了 refreshToken 过期校验，提升安全性",
      "confidence": 0.85,
      "tags": ["security", "bugfix"]
    },
    ...
  ]
}

注意：
- description 使用中文，简洁描述操作意图
- tags 从以下选择：feature, bugfix, refactor, docs, chore, security, performance
- confidence 0.0~1.0，过低（如 < 0.5）的分析结果不写入
```

## 二、心跳变更

### 新增：operation_log 心跳（5 分钟一次）

```rust
// heartbeat.rs 新增
pub fn start_operation_log_heartbeat(
    app: AppHandle,
    db: DbPool,
    settings: SettingsHandle,
) {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(300)); // 5 分钟

            let changes = collect_recent_file_changes(&db, 5); // 近 5 分钟变更
            if changes.is_empty() { continue; }

            let settings_snap = { settings.lock().unwrap().clone() };

            // 调用 LLM 分析
            if let Some(analysis) = call_intention_analysis(&settings_snap, &changes) {
                save_operation_logs(&db, &analysis, chunk_id);
            }

            // 触发主动建议检查
            check_proactive_suggestions(&app, &db, &settings_snap, &changes);
        }
    });
}
```

### 修改现有心跳

- 浅层心跳（30s）：保持不变，只监听文件变更写 `file_changes`
- 中层心跳（10min）：保持语义分析逻辑
- operation_log 心跳（5min）：新增，专门做意图分析
- 深层心跳（30min 检查）：保持日报生成

## 三、主动建议引擎

### 触发条件

**定时检查（每 30 分钟）：**
1. intent 文档今天是否已读（通过浅层心跳的 `check_intent_doc_update` 时间戳判断）
2. 消息队列是否有 P0/P1 积压超过阈值

**事件触发：**
```rust
// 关键模块关键词
const CRITICAL_MODULES = ["auth", "security", "password", "token", "payment", "config"];

fn detect_critical_changes(changes: &[FileChange]) -> Vec<Change> {
    changes.iter()
        .filter(|c| CRITICAL_MODULES.iter().any(|kw| c.path.contains(kw)))
        .collect()
}
```

### 气泡提醒展示

检测到关键模块变更时，通过 `Orb` 组件的气泡展示：

```tsx
// App.tsx 新增状态
const [agentAlert, setAgentAlert] = useState<{
  type: 'critical' | 'intent_reminder' | 'queue_warning';
  title: string;
  message: string;
} | null>(null);

// listen('agent:alert') 显示气泡
```

气泡样式：
- 类型标签：🔒 安全类 / 📋 意图提醒 / ⚠️ 队列积压
- 点击"查看"跳转主窗口对应 Tab
- "稍后"关闭，15 分钟内不重复提醒

## 四、对话功能

### 新增 Tab：对话

```
┌─────────────────────────────────┐
│  🔵 Auto-Heart              活跃 │
├─────────────────────────────────┤
│  今天                          │
│  语义地图                      │
│  对话 ← 新增                   │
│  设置                          │
├─────────────────────────────────┤
│                                 │
│  [对话历史列表]                 │
│                                 │
│  ┌───────────────────────────┐  │
│  │ 用户：今天下午我做了什么？  │  │
│  └───────────────────────────┘  │
│                                 │
│  ┌───────────────────────────┐  │
│  │ AI：根据 operation_log     │  │
│  │ 分析，你下午主要在处理     │  │
│  │ auth 模块的权限验证...     │  │
│  └───────────────────────────┘  │
│                                 │
├─────────────────────────────────┤
│ [输入框.....................]发送│
└─────────────────────────────────┘
```

### 对话命令支持

| 命令模式 | 示例 | 说明 |
|---------|------|------|
| 日志查询 | "今天下午我做了什么？" | 分析当天的 operation_log |
| 文件搜索 | "找一下上周修改的 auth 文件" | 搜索 file_changes |
| 趋势分析 | "这周每天平均多少个文件变更？" | 聚合统计 |
| 开放问答 | "基于今天的日志给我建议" | 综合分析 |

### 对话命令路由

```rust
fn route_conversation(user_input: &str) -> Command {
    if user_input.contains("今天") && user_input.contains("做了") {
        Command::QueryTodayLog
    } else if user_input.contains("找") && user_input.contains("文件") {
        Command::SearchFiles
    } else if user_input.contains("周") && user_input.contains("平均") {
        Command::TrendAnalysis
    } else {
        Command::FreeForm
    }
}
```

### Prompt 上下文注入

对话时将以下信息作为 system prompt 上下文：

```
当前日期：2026-03-26
用户今天的关键操作：
- 14:00 修改 auth/guard.ts → 添加 refreshToken 校验
- 15:30 修改 UserService.java → 实现 batch 接口
- 16:45 创建 docs/API.md → 补充接口文档

今天的任务：
- 10:00 refreshToken 过期校验 [进行中]
- 13:00 dashboard 接口联调 [待处理]

请基于以上信息回答用户问题。
```

## 五、前端变更

### 新增文件

- `src/pages/ConversationTab.tsx` — 对话 Tab 组件
- `src/components/ChatMessage.tsx` — 单条消息组件
- `src/hooks/useConversation.ts` — 对话状态管理

### 修改文件

- `src/App.tsx` — 注册 ConversationTab
- `src/pages/MainWindow.tsx` — Tab 列表增加"对话"
- `src/components/Orb.tsx` — 支持 agent 气泡提醒
- `src/hooks/useMessageQueue.ts` — 复用消息队列逻辑

### 命令接口

```rust
// commands.rs 新增

#[tauri::command]
async fn conversation_chat(
    message: String,
    db: State<'_, DbPool>,
    settings: State<'_, SettingsHandle>,
) -> Result<String, String> {
    // 路由 + 调用深层模型
}

#[tauri::command]
fn query_operation_logs(
    date: String,  // "2026-03-26" 或 "today"
    db: State<'_, DbPool>,
) -> Vec<OperationLog>;

#[tauri::command]
fn search_file_changes(
    keyword: String,
    date: Option<String>,
    db: State<'_, DbPool>,
) -> Vec<FileChange>;

#[tauri::command]
fn get_trend_stats(
    days: i32,  // 最近 N 天
    db: State<'_, DbPool>,
) -> TrendStats;
```

## 六、关键模块检测规则

```rust
// 关键模块关键词（不区分大小写）
const CRITICAL_PATTERNS: &[(&str, &str)] = &[
    ("auth", "认证授权模块"),
    ("security", "安全相关"),
    ("password", "密码处理"),
    ("token", "令牌验证"),
    ("payment", "支付相关"),
    ("config", "配置变更"),
    ("middleware", "中间件"),
    ("permission", "权限控制"),
];

fn classify_module(path: &str) -> Option<&'static str> {
    let path = path.to_lowercase();
    for (pattern, label) in CRITICAL_PATTERNS {
        if path.contains(pattern) {
            return Some(label);
        }
    }
    None
}
```

## 七、配置项（新增）

在 `AppSettings` 中新增：

```rust
pub struct AppSettings {
    // ... 现有字段 ...

    // 新增
    /// 主动建议开关
    pub proactive_suggestions: bool,
    /// 关键模块关键词（逗号分隔）
    pub critical_keywords: String,
    /// 对话 AI 模型（默认使用深层模型）
    pub chat_model: String,
    pub chat_model_name: String,
}
```

## 验证方案

1. `npm run tauri dev` 启动
2. 手动触发一些文件变更，等待 5 分钟观察 `operation_log` 表
3. 测试对话 Tab："今天我做了什么？"
4. 修改 auth 相关文件，观察是否出现气泡提醒
5. 检查每日数据目录下的数据库是否正确创建

## 风险与限制

1. **Token 消耗**：5 分钟一次 LLM 调用，每天约 144 次。需要确保消息队列的沉默阈值过滤掉低优先级问题。
2. **LLM 幻觉**：意图分析可能不准确，confidence 过低的跳过不写入。
3. **性能**：大量文件变更时（如 git pull），批量处理可能耗时较长。
