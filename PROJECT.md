# Auto-Heart 项目理解文档

> 存在于你工作环境边缘的生命体 —— 一个会呼吸的桌面助手

## 项目概述

**Auto-Heart** 是一个 Tauri 桌面应用，核心是一个会呼吸的小球（Orb），浮在屏幕边缘感知开发者的日常工作。

### 核心价值
- 感知：监听文件变更、活跃应用、意图文档
- 理解：通过 LLM 分析变更意图，构建语义地图
- 回应：在合适的时机（自然节点）推送建议或日报

---

## 技术架构

### 技术栈
| 层级 | 技术 |
|------|------|
| 桌面框架 | Tauri 2.x (Rust + WebView2) |
| 前端 | React 18 + TypeScript + Vite |
| 后端 | Rust |
| 数据库 | SQLite (rusqlite, WAL 模式) |
| AI 模型 | OpenAI-compatible API (Kimi/Qwen/Claude/GPT/DeepSeek/Ollama) |

### 项目结构
```
auto-heart/
├── src/                      # 前端 (React)
│   ├── components/           # UI 组件
│   │   ├── Orb.tsx          # 呼吸小球
│   │   └── SpeechBubble.tsx # 气泡组件
│   ├── pages/                # 页面
│   │   ├── MainWindow.tsx   # 主窗口
│   │   ├── TodayTab.tsx      # 今日任务
│   │   ├── SemanticMapTab.tsx # 语义地图
│   │   ├── ConversationTab.tsx # 日志查询
│   │   └── SettingsTab.tsx   # 设置
│   ├── hooks/                # React hooks
│   └── App.tsx               # Orb 小球入口
├── src-tauri/               # 后端 (Rust)
│   └── src/
│       ├── lib.rs           # 主入口、托盘、三层心跳启动
│       ├── heartbeat.rs     # 心跳调度核心
│       ├── model_router.rs  # LLM 路由 (多提供商)
│       ├── database.rs      # SQLite schema
│       ├── commands.rs      # Tauri IPC 命令
│       ├── settings.rs      # 配置管理
│       └── notifier.rs      # 钉钉/飞书通知
└── docs/                    # 设计文档
```

---

## 核心概念

### 1. 三层心跳机制

| 层级 | 频率 | 模型 | 功能 |
|------|------|------|------|
| 浅层 | 30秒 | 无 | 活跃应用检测、意图文档监听、自然节点释放 P1 消息 |
| 中层 | 10分钟 | 轻量模型 (Kimi/Qwen) | 文件变更语义分析、意图文档解析、消息队列更新 |
| 深层 | 30分钟检查 | 强模型 (Claude/GPT) | 下班检测、日报生成 |

### 2. 语义地图 (Semantic Map)

存储模块级别的代码理解：
- `semantic_modules`: 模块名、描述、依赖、业务含义理解
- `decision_log`: 技术决策记录
- `tech_debt`: 技术债务跟踪

### 3. 消息队列与沉默模式

| 模式 | 行为 |
|------|------|
| `open` | 接收所有消息 |
| `normal` | 只拦截 P0/P1 |
| `focus` | 只拦截 P0 |

**自然节点**: 用户停止打字 90 秒后，释放 P1 消息。

### 4. 意图文档

用户在工作区放置意图文档（Markdown），Auto-Heart 监测变更并：
1. 读取文档内容
2. 中层心跳调用 LLM 解析为结构化任务
3. 关联代码模块
4. 推送任务提醒

### 5. 模型路由器

支持 8 个提供商自动路由：
- `kimi` → moonshot-v1-8k
- `qwen` → qwen-plus
- `minimax` → abab6.5s-chat
- `gpt/openai` → gpt-4o-mini
- `claude` → claude-sonnet-4-5
- `deepseek` → deepseek-chat
- `openrouter` → 统一代理
- `ollama` → 本地模型

---

## 数据库 Schema

```
┌─────────────────────┬──────────────────────────────────────┐
│ 表名                │ 用途                                  │
├─────────────────────┼──────────────────────────────────────┤
│ semantic_modules    │ 模块理解记录                          │
│ decision_log        │ 决策日志                              │
│ tech_debt           │ 技术债记录                            │
│ intent_history      │ 意图历史（raw_text + parsed_tasks）   │
│ message_queue       │ 消息队列（priority 0/1/2）            │
│ file_changes        │ 文件变更记录（监听写入）              │
│ operation_log       │ 操作日志（LLM 意图分析）              │
│ daily_reports       │ 日报（draft/confirmed/sent）         │
└─────────────────────┴──────────────────────────────────────┘
```

---

## 前端 UI

### Orb 小球 (App.tsx)
三种状态：
- `idle`: 沉默呼吸
- `thinking`: 思考跳动（中层心跳触发）
- `speaking`: 气泡浮现（新消息/日报/建议）

### 主窗口 (MainWindow)
Tab 页面：
- **今日**: 今日任务 + 意图文档
- **语义地图**: 模块列表 + 决策日志
- **对话**: 操作日志查询 + 文件搜索 + 趋势统计
- **设置**: API Key 配置、沉默模式、监听路径等

---

## 关键流程

### 日循环
1. **早晨**: 读取意图文档
2. **白天**: 文件监听 → 语义分析 → 自然节点发言
3. **下班**: 生成日报 → 推送 → 发送至钉钉/飞书

### 消息推送流程
```
文件变更 → 中层心跳分析 → P0 立即推送
                          → P1 等 90s 无操作后推送
                          → P2 仅在日报中体现
```

---

## 配置项 (AppSettings)

```rust
struct AppSettings {
    // AI 模型配置
    middle_model: String,       // 中层模型提供商
    deep_model: String,        // 深层模型提供商

    // API Keys (kimi/qwen/minimax/gpt/claude/deepseek/openrouter)
    kimi_api_key: String,

    // 行为控制
    silence_mode: String,      // open/normal/focus
    proactive_suggestions: bool,
    critical_keywords: String, // 关键模块关键词

    // 路径配置
    intent_doc_path: String,    // 意图文档路径
    data_dir: String,           // 数据存储目录
    watch_paths: Vec<String>,  // 监听目录

    // 下班与通知
    offwork_time: String,       // 下班时间
    dingtalk_webhook: String,
    feishu_webhook: String,
}
```

---

## 最近更新 (2026-03)

- `feat(frontend)`: 支持 Agent 主动建议气泡 (`agent:alert`)
- `feat(heartbeat)`: 添加主动建议引擎
- `feat(frontend)`: ConversationTab 日志查询 UI
- `feat(commands)`: 日志查询、文件搜索、趋势统计命令

---

## 运行与构建

```bash
# 开发
npm run tauri dev

# 构建
npm run tauri build

# 或
npm run release
```
