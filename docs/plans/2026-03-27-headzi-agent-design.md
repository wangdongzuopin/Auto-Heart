# 头子智能体设计方案

## 概述

将主窗口对话功能升级为有记忆、有名字的智能体"头子"，具备跨会话记忆、流式思考展示、智能数据查询能力。

## 需求清单

| 需求 | 描述 |
|------|------|
| AI 名字 | 头子，自称"头子"，以这个名字与用户对话 |
| 思考展示 | 思考内容用浅灰色显示，流式输出，默认折叠，点击展开/收起 |
| 流式输出 | AI 回复实时流式返回，前端逐字显示 |
| 跨会话记忆 | 启动时读取 `conversations/` 历史，拼接为上下文 |
| 角色记忆 | 启动时读取 `memory.md` 作为固定背景设定 |
| 智能查询 | 用户说"今天我做了什么"时，自动读取当日数据目录 |
| 统一处理 | 所有对话统一由头子处理，去掉意图关键词解析 |

## 数据结构

```
{app_data_dir}/
├── memory.md              # 头子的角色记忆（手动编辑，UTF-8）
├── conversations/         # 历史会话目录
│   ├── {uuid}.json
│   └── ...
└── YYYY-MM-DD/           # 每日数据目录（由心跳模块写入）
    ├── intent_history.json
    ├── operation_logs.json
    └── ...
```

## System Prompt 构建

### 启动时构建流程

1. 读取 `memory.md`（不存在则跳过）
2. 读取 `conversations/` 下所有会话，按时间倒序，取最近 N 条（如50条）
3. 拼接为上下文
4. 注入 `system_prompt`

### System Prompt 示例

```
你是头子，一个栖息在用户屏幕边缘的生命体。你能感知用户的工作状态、阅读意图文档、观察文件变化，并以温柔而克制的方式与用户交流。

【你的记忆】
（memory.md 的内容）

【近期对话】
2026-03-25: 用户问项目进度，头子回答了番茄工作法的建议
2026-03-26: 用户说"今天状态不错"，头子鼓励了他

当用户说"今天我做了什么"时，自动从今日数据目录读取记录并汇总。
```

## 智能查询行为

### 触发条件

用户消息匹配以下模式之一：
- 包含"今天我做了"
- 包含"今天干了"
- 包含"今日工作"
- 包含"我今天"

### 执行流程

1. 检测到触发 → 构造 system prompt（带查询指令）
2. 读取 `{data_dir}/YYYY-MM-DD/` 目录下所有文件
3. 拼接文件内容作为 context
4. 调用模型时注入 context，请求汇总今日工作

## 前端 UI 变化

### 消息气泡结构

```tsx
// 助手消息
<div class="message assistant">
  {/* 折叠/展开控制 */}
  <div class="thinking-toggle" onClick={toggleThinking}>
    头子在思考 {collapsed ? '▼' : '▲'}
  </div>

  {/* 思考内容（流式，浅灰色） */}
  {!collapsed && (
    <div class="thinking-content">
      {thinkingText}
    </div>
  )}

  {/* 正式回答（流式） */}
  <div class="answer-content">
    {answerText}
  </div>
</div>
```

### 样式

```css
.thinking-content {
  color: #9ca3af;          /* 浅灰色 */
  font-size: 12px;
  padding: 4px 0;
  white-space: pre-wrap;
}

.thinking-toggle {
  color: #6b7280;
  font-size: 11px;
  cursor: pointer;
  margin-bottom: 4px;
}

.answer-content {
  color: var(--color-text-primary);
  font-size: 13px;
}
```

### 流式渲染

使用 `invoke` 的 SSE（Server-Sent Events）或流式 `fetch`，逐字追加到 state。

## Rust 端变化

### 新增字段

```rust
// AppSettings 新增
pub agent_name: String,           // 默认 "头子"
pub agent_memory_path: String,    // 空 = 使用 data_dir/memory.md
```

### 新增命令

```rust
// 读取记忆和历史，构建 system prompt
#[tauri::command]
pub fn build_system_prompt(
    settings: State<'_, SettingsHandle>,
) -> Result<String, String>

// 读取今日数据目录
#[tauri::command]
pub fn read_today_data() -> Result<String, String>

// 流式发送消息（保留原接口，前端决定是否流式）
```

### ModelRouter 变化

`ModelRouter` 新增流式调用方法：

```rust
pub async fn call_with_config_streaming(
    &self,
    config: &ModelConfig,
    messages: &[OaiMessage],
    system: Option<&str>,
) -> impl Stream<Item = String>
```

## 移除逻辑

- `parse_intent_from_chat` 调用（在 `send_message` 中）
- `contains_intent_keywords` 函数
- 意图关键词数据库写入逻辑

## 文件变更清单

| 文件 | 变更 |
|------|------|
| `src-tauri/src/model_router.rs` | 新增流式调用方法 |
| `src-tauri/src/commands.rs` | 新增 `build_system_prompt`、`read_today_data` 命令；移除意图解析逻辑 |
| `src-tauri/src/settings.rs` | 新增 `agent_name`、`agent_memory_path` 字段 |
| `src/pages/ConversationTab.tsx` | 重构为流式渲染 + 思考折叠 UI |
| `src/hooks/useSettings.ts` | 新增 agent 相关字段同步 |
| `docs/plans/2026-03-27-headzi-agent-design.md` | 本文档 |
