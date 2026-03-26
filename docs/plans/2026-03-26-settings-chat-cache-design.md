# 设置持久化、窗口缓存与对话功能设计

## 1. 概述

本次设计解决三个需求：
1. **设置持久化** —— JSON 配置文件，程序启动时读取
2. **窗口位置缓存** —— 小球窗口和主窗口位置/大小持久化
3. **日常对话功能** —— 主窗口对话 Tab + 持久化聊天历史

---

## 2. JSON 配置文件

### 2.1 文件路径

```
{data_dir}/settings.json
```

其中 `data_dir` 为 `tauri-plugin-store` 中的 `data_dir` 配置项（默认为 `app_data_dir`）。

### 2.2 Schema

```json
{
  "data_dir": "C:/Users/.../AppData/...",
  "middle_model": "kimi",
  "middle_model_name": "moonshot-v1-8k",
  "deep_model": "claude",
  "deep_model_name": "claude-sonnet-4-5",

  "silence_mode": "normal",
  "proactive_suggestions": true,
  "critical_keywords": "auth,payment,security",

  "watch_paths": ["D:/Projects/myapp"],
  "offwork_time": "18:00",

  "dingtalk_webhook": "",
  "feishu_webhook": "",

  "chat_model": "kimi",
  "chat_model_name": "moonshot-v1-8k"
}
```

### 2.3 配置读写

**Rust 侧** (`settings.rs`)：
- 新增 `load_settings_from_file(path: &PathBuf) -> AppSettings`
- 新增 `save_settings_to_file(path: &PathBuf, settings: &AppSettings)`
- 程序启动时读取 `settings.json`，作为默认配置
- 用户保存设置时，同时更新内存和写回 `settings.json`

**移除**：
- `intent_doc_path` 配置项（不再使用）

**新增**：
- `chat_model` / `chat_model_name` —— 对话专用模型配置

---

## 3. 窗口位置缓存

### 3.1 小球窗口

存储 key：`auto-heart:orb-pos-v3`
```json
{ "x": 1700, "y": 900, "width": 120, "height": 120 }
```

已有实现（`App.tsx` + `localStorage`），无需修改。

### 3.2 主窗口

存储 key：`auto-heart:main-window-pos-v1`
```json
{ "x": 100, "y": 100, "width": 480, "height": 680, "isMaximized": false }
```

**实现方式**：Rust 命令保存窗口状态
- `save_window_state` 命令：在主窗口关闭时调用，写入 `settings.json`
- `restore_window_state` 命令：主窗口打开时调用，恢复位置

---

## 4. 对话功能

### 4.1 数据存储

对话历史存储在 `{data_dir}/conversations/` 目录。

每个 Session 一个文件：
```
{data_dir}/conversations/{session_id}.json
```

### 4.2 Session 文件结构

```json
{
  "id": "uuid",
  "title": "对话标题（取首条用户消息前20字）",
  "created_at": "2026-03-26T10:00:00",
  "updated_at": "2026-03-26T10:30:00",
  "messages": [
    {
      "id": "uuid",
      "role": "user",
      "content": "今天要做XXX",
      "timestamp": "2026-03-26T10:00:00"
    },
    {
      "id": "uuid",
      "role": "assistant",
      "content": "好的，我已记录...",
      "timestamp": "2026-03-26T10:00:05"
    }
  ]
}
```

### 4.3 意图解析触发

当用户消息中包含任务相关意图时（如"今天要做"、"待办"、"计划"），自动：
1. 调用中层模型解析任务
2. 写入 `intent_history` 表
3. 前端收到 `intent:parsed` 事件，刷新今日任务

### 4.4 对话 API

**Rust 命令**：

| 命令 | 功能 |
|------|------|
| `get_conversations` | 获取会话列表 |
| `get_conversation(id)` | 获取单个会话（含消息） |
| `create_conversation` | 创建新会话 |
| `delete_conversation(id)` | 删除会话 |
| `send_message(session_id, content)` | 发送消息，返回 AI 回复 |
| `parse_intent_from_chat(content)` | 从消息内容解析意图 |

**前端 UI**：
- 对话 Tab 显示会话列表 + 当前会话消息
- 气泡输入框 + 发送按钮
- 新建会话 / 删除会话按钮

### 4.5 小球触发

点击小球 → `open_main_window` → 前端自动聚焦对话 Tab（通过 URL 参数 `?view=chat` 或状态管理）。

---

## 5. 监听目录配置

### 5.1 默认行为

`watch_paths` 数组为空时，自动检测并添加：
- Windows: `Documents`、`Desktop`、`Code`、`Projects`
- 检测到子目录包含 `.git` 或 `package.json`，自动加入监听

### 5.2 多选支持

`watch_paths` 支持多个路径，前端 UI 提供目录选择器（可添加/删除多个目录）。

---

## 6. 实现步骤

### Phase 1: 设置持久化
1. 修改 `settings.rs` —— JSON 文件读写
2. 更新 `AppSettings` 结构体 —— 移除 `intent_doc_path`，新增 `chat_model`
3. 修改 Rust 启动逻辑 —— 从 `settings.json` 加载配置
4. 修改 `save_settings` 命令 —— 同时写回 JSON 文件

### Phase 2: 窗口缓存
1. 新增 `save_window_state` / `restore_window_state` 命令
2. 前端主窗口关闭时调用保存
3. 主窗口打开时调用恢复

### Phase 3: 对话功能
1. 创建 `conversation.rs` —— 对话 CRUD
2. 新增 Tauri 命令
3. 前端对话 Tab UI
4. 小球点击触发逻辑

### Phase 4: 监听目录优化
1. 自动检测常见开发目录
2. 前端目录多选 UI
