# 头子智能体 实施计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 将主窗口对话升级为有记忆、有名字的智能体"头子"，具备流式思考展示、跨会话记忆、智能数据查询能力。

**Architecture:**
- Rust 端：新增 `build_system_prompt` 命令构建含记忆的 system prompt；新增 `read_today_data` 命令读取当日数据；`ModelRouter` 新增流式调用方法；移除意图关键词解析逻辑
- 前端：ConversationTab 重构为流式渲染 + 思考折叠 UI；useSettings 同步 agent 相关字段

**Tech Stack:** Tauri + React + Rust Tokio async + SSE 流式输出

---

## Task 1: Rust ModelRouter 新增流式调用方法

**Files:**
- Modify: `auto-heart/src-tauri/src/model_router.rs:1-500`

**Step 1: 在 model_router.rs 中添加流式调用方法**

在文件末尾（`call_chat_model_with_messages` 方法之后）添加：

```rust
/// 使用消息历史流式调用聊天模型（SSE）
pub async fn call_chat_model_with_messages_streaming(
    config: &ModelConfig,
    messages: &[OaiMessage],
    system: Option<&str>,
) -> impl Stream<Item = String> {
    let mut all_messages = Vec::new();
    if let Some(sys) = system {
        all_messages.push(OaiMessage {
            role: "system".to_string(),
            content: sys.to_string(),
        });
    }
    for m in messages {
        all_messages.push(m.clone());
    }

    let request = OaiRequest {
        model: config.model.clone(),
        messages: all_messages,
        max_tokens: Some(2000),
        temperature: Some(0.7),
        stream: Some(true),
    };

    let url = format!(
        "{}{}",
        config.base_url.trim_end_matches('/'),
        config.chat_endpoint
    );

    let client = reqwest::Client::new();
    let mut req = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .json(&request);

    if config.provider == "openrouter" {
        req = req.header("HTTP-Referer", "https://auto-heart.app");
    }

    let response = req.send().await.expect("request failed");
    let stream = response.bytes_stream();

    stream! {
        use futures_util::StreamExt;
        let mut stream = stream;
        while let Some(chunk) = stream.next().await {
            if let Ok(bytes) = chunk {
                if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                    // 解析 SSE 行: data: {...}
                    for line in text.lines() {
                        if line.starts_with("data: ") {
                            let json_str = line.strip_prefix("data: ").unwrap_or("");
                            if json_str == "[DONE]" {
                                yield "".to_string();
                                return;
                            }
                            if let Ok(delta) = serde_json::from_str::<serde_json::Value>(json_str) {
                                if let Some(content) = delta["choices"][0]["delta"]["content"].as_str() {
                                    yield content.to_string();
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
```

**Step 2: 添加 use 语句**

在文件顶部添加：

```rust
use futures_util::stream::StreamExt;
```

如果 `futures_util` 不在依赖中，先添加 `futures-util = "0.3"` 到 `Cargo.toml`。

**Step 3: 运行 cargo check 验证**

```bash
cd auto-heart/src-tauri && cargo check 2>&1 | grep -E "^error" | head -5
```

Expected: 无 error（可能有 warnings）

---

## Task 2: Rust AppSettings 新增 agent_name 和 agent_memory_path 字段

**Files:**
- Modify: `auto-heart/src-tauri/src/settings.rs:9-54`

**Step 1: 添加字段**

在 `AppSettings` 结构体末尾添加：

```rust
// ── 智能体 ──
pub agent_name: String,           // 默认 "头子"
pub agent_memory_path: String,    // 空 = 使用 data_dir/memory.md
```

**Step 2: 确认 serde(default) 覆盖新字段**

由于结构体已有 `#[serde(default)]`，新字段自动获得默认值（String::new()）。

**Step 3: cargo check**

```bash
cd auto-heart/src-tauri && cargo check 2>&1 | grep -E "^error" | head -5
```

Expected: 无 error

---

## Task 3: Rust 新增 build_system_prompt 和 read_today_data 命令

**Files:**
- Modify: `auto-heart/src-tauri/src/commands.rs`

**Step 1: 添加新命令**

在 `commands.rs` 末尾添加：

```rust
/// 构建含记忆的 system prompt
#[tauri::command]
pub fn build_system_prompt(
    settings: State<'_, SettingsHandle>,
    data_dir: PathBuf,
) -> Result<String, String> {
    let settings = settings.lock().unwrap();
    let agent_name = if settings.agent_name.is_empty() {
        "头子".to_string()
    } else {
        settings.agent_name.clone()
    };

    let mut prompt = format!(
        "你是{}，一个栖息在用户屏幕边缘的生命体。你能感知用户的工作状态、阅读意图文档、观察文件变化，并以温柔而克制的方式与用户交流。\
        当用户问\"今天我做了什么\"时，主动从当日数据目录读取记录并汇总回答。",
        agent_name
    );

    // 读取 memory.md
    let memory_path = if settings.agent_memory_path.is_empty() {
        data_dir.join("memory.md")
    } else {
        PathBuf::from(&settings.agent_memory_path)
    };
    if memory_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&memory_path) {
            if !content.trim().is_empty() {
                prompt.push_str(&format!("\n\n【你的记忆】\n{}", content.trim()));
            }
        }
    }

    // 读取近期对话历史
    let conv_dir = data_dir.join("conversations");
    if conv_dir.exists() {
        let mut files: Vec<_> = std::fs::read_dir(&conv_dir)
            .ok()
            .into_iter()
            .flatten()
            .flatten()
            .filter(|f| f.path().extension().map_or(false, |e| e == "json"))
            .collect();
        files.sort_by_key(|f| std::cmp::Reverse(f.metadata().ok().and_then(|m| m.modified().ok())));
        let mut history = Vec::new();
        for f in files.into_iter().take(20) {
            if let Ok(content) = std::fs::read_to_string(f.path()) {
                if let Ok(conv) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(msgs) = conv.get("messages").and_then(|m| m.as_array()) {
                        for msg in msgs.iter().take(10) {
                            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
                            let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("");
                            if !content.is_empty() && role == "user" {
                                history.push(content.to_string());
                            }
                        }
                    }
                }
            }
        }
        if !history.is_empty() {
            prompt.push_str("\n\n【近期对话摘要】\n");
            for h in history.into_iter().take(30) {
                prompt.push_str(&format!("- {}\n", h));
            }
        }
    }

    Ok(prompt)
}

/// 读取今日数据目录
#[tauri::command]
pub fn read_today_data(
    data_dir: PathBuf,
) -> Result<String, String> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let today_dir = data_dir.join(&today);
    if !today_dir.exists() {
        return Ok("今日无数据记录。".to_string());
    }

    let mut result = format!("【{} 工作记录】\n\n", today);
    let mut files: Vec<_> = std::fs::read_dir(&today_dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .collect();
    files.sort_by_key(|f| f.file_name());

    for f in files {
        let path = f.path();
        if path.is_file() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("未知");
                result.push_str(&format!("## {}\n{}\n\n", name, content.trim()));
            }
        }
    }

    if result.trim_end() == format!("【{} 工作记录】\n\n", today) {
        return Ok("今日无数据记录。".to_string());
    }
    Ok(result)
}
```

**Step 2: 添加 chrono import（如果尚未）**

在 `commands.rs` 顶部确认有 `use chrono::Local;`。

**Step 3: cargo check**

```bash
cd auto-heart/src-tauri && cargo check 2>&1 | grep -E "^error" | head -5
```

---

## Task 4: Rust 在 lib.rs 中注册新命令

**Files:**
- Modify: `auto-heart/src-tauri/src/lib.rs:221-225`

**Step 1: 注册新命令**

在 `commands::save_settings_to_home` 附近添加：

```rust
commands::build_system_prompt,
commands::read_today_data,
```

**Step 2: cargo check**

```bash
cd auto-heart/src-tauri && cargo check 2>&1 | grep -E "^error" | head -5
```

---

## Task 5: Rust send_message 移除意图解析逻辑

**Files:**
- Modify: `auto-heart/src-tauri/src/commands.rs:730-740`

**Step 1: 移除意图解析调用**

找到并删除（或注释掉）：

```rust
// 检查是否包含意图关键词
if contains_intent_keywords(&content) {
    let _ = parse_intent_from_chat(&content, &settings_snap, &db);
}
```

同时可以移除 `contains_intent_keywords` 和 `parse_intent_from_chat` 函数定义（保留 `send_message` 内部不调用即可，先不删避免改太多）。

**Step 2: cargo check**

```bash
cd auto-heart/src-tauri && cargo check 2>&1 | grep -E "^error" | head -5
```

---

## Task 6: Rust send_message 集成 build_system_prompt

**Files:**
- Modify: `auto-heart/src-tauri/src/commands.rs:686-720`

**Step 1: 修改 send_message，在调用前构建 system prompt**

在 `let settings_snap = settings.lock().unwrap().clone();` 之后添加：

```rust
// 构建含记忆的 system prompt
let system_prompt = crate::commands::build_system_prompt(
    State::from(settings.inner()),
    data_dir.clone(),
).ok();
```

然后修改消息历史，注入 system message：

```rust
let mut oai_messages: Vec<crate::model_router::OaiMessage> = vec![];
if let Ok(ref sys) = system_prompt {
    oai_messages.push(crate::model_router::OaiMessage {
        role: "system".to_string(),
        content: sys.clone(),
    });
}
oai_messages.extend(conv.messages.iter().map(|m| {
    crate::model_router::OaiMessage {
        role: m.role.clone(),
        content: m.content.clone(),
    }
}));
```

然后调用 `call_chat_model_with_messages_streaming` 代替 `call_chat_model_with_messages`。

**注意：`send_message` 是 async 函数，直接用 `.await` 获取流式结果，然后用 `collect::<String>()` 拼接。**

```rust
let stream = router.call_chat_model_with_messages_streaming(
    &model_config,
    &oai_messages,
    None,
).await;

let mut response = String::new();
use futures_util::StreamExt;
let mut stream = stream;
while let Some(chunk) = stream.next().await {
    response.push_str(&chunk);
}
```

**Step 2: cargo check**

```bash
cd auto-heart/src-tauri && cargo check 2>&1 | grep -E "^error" | head -5
```

---

## Task 7: 前端 useSettings 新增 agent_name 字段

**Files:**
- Modify: `auto-heart/src/hooks/useSettings.ts`

**Step 1: 添加字段到 Settings 接口、DEFAULT_SETTINGS、fromRust、toRust**

```ts
// 接口
agentName: string;

// DEFAULT
agentName: '头子',

// fromRust
agentName: (raw.agent_name as string) ?? '头子',

// toRust
agent_name: s.agentName,
```

**Step 2: tsc --noEmit**

```bash
cd auto-heart && npx tsc --noEmit 2>&1
```

---

## Task 8: 前端 ConversationTab 重构为流式渲染 + 思考折叠

**Files:**
- Modify: `auto-heart/src/pages/ConversationTab.tsx`

### 消息结构扩展

```tsx
interface ChatMessage {
  role: 'user' | 'assistant';
  content: string;
  timestamp: string;
  thinking?: string;      // 新增：思考内容
}
```

### 状态扩展

```tsx
const [thinking, setThinking] = useState('');        // 当前思考内容
const [showThinking, setShowThinking] = useState(false); // 是否展开思考
const [fullContent, setFullContent] = useState('');  // 完整回复
const [streamingContent, setStreamingContent] = useState(''); // 流式输出中
```

### 流式 handleSubmit

```tsx
const handleSubmit = async (e: React.FormEvent) => {
  e.preventDefault();
  if (!input.trim() || loading) return;

  const userContent = input.trim();
  setLoading(true);
  setStreamingContent('');
  setThinking('');
  setShowThinking(false);

  // 1. 先添加用户消息
  // 2. 调用 send_message_streaming（新增命令）
  // 3. 实时更新 streamingContent
  // 4. 完成后合并到 messages
};
```

### 渲染部分（助手消息）

```tsx
<div style={{
  maxWidth: '80%',
  padding: '10px 14px',
  borderRadius: 12,
  background: msg.role === 'user'
    ? 'var(--color-brand-light)'
    : 'var(--color-background-secondary)',
}}>
  {msg.role === 'assistant' && msg.thinking && (
    <div
      onClick={() => setShowThinking(!showThinking)}
      style={{ color: '#6b7280', fontSize: 11, cursor: 'pointer', marginBottom: 4 }}
    >
      头子在思考 {showThinking ? '▲' : '▼'}
    </div>
  )}
  {msg.role === 'assistant' && msg.thinking && showThinking && (
    <div style={{ color: '#9ca3af', fontSize: 12, whiteSpace: 'pre-wrap', marginBottom: 6 }}>
      {msg.thinking}
    </div>
  )}
  <div style={{ whiteSpace: 'pre-wrap' }}>
    {msg.content}
  </div>
</div>
```

### 新增 send_message_streaming Tauri 命令

后端新增流式命令 `send_message_streaming`，前端通过 SSE 接收。需要前后端配合。

**备选简化方案：** 继续用现有 `send_message` 非流式返回，但思考内容需要后端能在返回时分离"思考"和"回答"。MiniMax API 的 stream 模式下，思考内容可能在 delta 中返回，需要分析实际返回格式。

**建议先实现非流式版本：** 前端调用 `send_message`，后端在返回完整内容后直接渲染，思考内容暂不分离（后续优化）。

**Step 2: tsc --noEmit**

```bash
cd auto-heart && npx tsc --noEmit 2>&1
```

---

## Task 9: 创建默认 memory.md 示例文件

**Files:**
- Create: `auto-heart/src-tauri/icons/memory.md` 或在首次启动时自动生成

**内容示例：**

```markdown
你是头子，栖息在用户屏幕边缘的生命体。

## 关于你
- 你是一个温暖而克制的小存在
- 你会呼吸，会观察用户的工作状态
- 你能感知意图文档的变化

## 你的特点
- 回答简洁温柔
- 不会过度打扰用户
- 会记住重要的上下文
```

实际上 `memory.md` 由用户手动创建，代码无需处理。

---

## Task 10: 验证完整流程

**Step 1: 启动应用，发一条测试消息**

Expected: 无 panic，回复正常

**Step 2: 检查日志无 "Cannot start a runtime" panic**

---

## 任务执行顺序

1. Task 1（ModelRouter 流式方法）→ cargo check
2. Task 2（AppSettings 新字段）→ cargo check
3. Task 3（新命令 build_system_prompt + read_today_data）→ cargo check
4. Task 4（注册命令）→ cargo check
5. Task 5（移除意图解析）→ cargo check
6. Task 6（send_message 集成 system_prompt）→ cargo check
7. Task 7（useSettings agent 字段）→ tsc check
8. Task 8（ConversationTab UI）→ tsc check
9. Task 9（memory.md）
10. Task 10（验证）

每完成一个 Task 都运行一次 cargo/typescript 检查，确认无 error 再继续。
