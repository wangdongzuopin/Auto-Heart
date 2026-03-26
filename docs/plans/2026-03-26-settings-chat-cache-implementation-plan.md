# 设置持久化、窗口缓存与对话功能实现计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 实现设置 JSON 持久化、主窗口位置缓存、日常对话功能

**Architecture:**
- Rust 后端：扩展 `settings.rs` 支持 JSON 文件读写，新增 `conversation.rs` 处理对话 CRUD
- 前端：新增对话 Tab UI，修改设置 Tab UI
- 数据：对话存储在 `{data_dir}/conversations/` 目录，每个 Session 一个 JSON 文件

**Tech Stack:** Tauri 2.x, React, Rust, SQLite (rusqlite), JSON (serde_json)

---

## Phase 1: 设置持久化

### Task 1: 修改 AppSettings 结构体

**Files:**
- Modify: `auto-heart/src-tauri/src/settings.rs`

**Step 1: 读取当前 settings.rs 内容**

```rust
// 当前 AppSettings 结构体移除 intent_doc_path，新增 chat_model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub data_dir: String,
    pub middle_model: String,
    pub middle_model_name: String,
    pub deep_model: String,
    pub deep_model_name: String,

    pub silence_mode: String,
    pub proactive_suggestions: bool,
    pub critical_keywords: String,

    pub watch_paths: Vec<String>,
    pub offwork_time: String,

    pub dingtalk_webhook: String,
    pub feishu_webhook: String,

    // 新增
    pub chat_model: String,
    pub chat_model_name: String,

    // 移除: intent_doc_path
}
```

**Step 2: 更新 default_settings 函数**

添加 `chat_model: "kimi".to_string()` 和 `chat_model_name: "moonshot-v1-8k".to_string()`

**Step 3: 提交**
```bash
git add auto-heart/src-tauri/src/settings.rs
git commit -m "refactor(settings): remove intent_doc_path, add chat_model"
```

---

### Task 2: 新增 JSON 文件读写函数

**Files:**
- Modify: `auto-heart/src-tauri/src/settings.rs`

**Step 1: 添加 load_settings_from_file 函数**

```rust
pub fn load_settings_from_file(path: &Path) -> Option<AppSettings> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}
```

**Step 2: 添加 save_settings_to_file 函数**

```rust
pub fn save_settings_to_file(path: &Path, settings: &AppSettings) -> Result<(), String> {
    let content = serde_json::to_string_pretty(settings)
        .map_err(|e| e.to_string())?;
    std::fs::write(path, content).map_err(|e| e.to_string())
}
```

**Step 3: 创建 get_settings_path 函数返回 settings.json 路径**

```rust
pub fn get_settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join("settings.json")
}
```

**Step 4: 提交**
```bash
git add auto-heart/src-tauri/src/settings.rs
git commit -m "feat(settings): add JSON file load/save functions"
```

---

### Task 3: 修改 lib.rs 启动逻辑加载 settings.json

**Files:**
- Modify: `auto-heart/src-tauri/src/lib.rs`

**Step 1: 在 setup 中读取 settings.json**

```rust
// 尝试从 settings.json 加载配置，如果失败则使用默认配置
let settings_path = settings::get_settings_path(&app_data_dir);
let app_settings = if settings_path.exists() {
    settings::load_settings_from_file(&settings_path)
        .unwrap_or_else(|| {
            eprintln!("[settings] 加载 settings.json 失败，使用默认配置");
            AppSettings::default()
        })
} else {
    // 首次运行，创建默认配置
    let default_settings = AppSettings::default();
    if let Err(e) = settings::save_settings_to_file(&settings_path, &default_settings) {
        eprintln!("[settings] 首次保存 settings.json 失败: {}", e);
    }
    default_settings
};
```

**Step 2: 修改 save_settings 命令**

```rust
#[tauri::command]
pub fn save_settings(
    app: AppHandle,
    new_settings: AppSettings,
    settings: State<'_, SettingsHandle>,
) -> Result<(), String> {
    let app_data_dir: PathBuf = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let settings_path = settings::get_settings_path(&app_data_dir);

    // 保存到文件
    settings::save_settings_to_file(&settings_path, &new_settings)?;

    // 更新内存
    let mut s = settings.lock().unwrap();
    *s = new_settings;
    Ok(())
}
```

**Step 3: 提交**
```bash
git add auto-heart/src-tauri/src/lib.rs
git commit -m "feat(settings): load settings.json on startup"
```

---

## Phase 2: 窗口缓存

### Task 4: 新增窗口状态结构体和命令

**Files:**
- Modify: `auto-heart/src-tauri/src/settings.rs`
- Modify: `auto-heart/src-tauri/src/commands.rs`
- Modify: `auto-heart/src-tauri/src/lib.rs`

**Step 1: 在 settings.rs 添加 WindowState**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub is_maximized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowCache {
    pub main: Option<WindowState>,
    pub orb: Option<WindowState>,
}
```

**Step 2: 在 commands.rs 添加命令**

```rust
#[tauri::command]
pub fn save_window_state(window_type: String, state: WindowState, settings: State<'_, SettingsHandle>) -> Result<(), String> {
    let mut s = settings.lock().unwrap();
    match window_type.as_str() {
        "main" => s.main_window_state = Some(state),
        "orb" => s.orb_window_state = Some(state),
        _ => return Err("Invalid window type".to_string()),
    }
    Ok(())
}

#[tauri::command]
pub fn load_window_state(window_type: String, settings: State<'_, SettingsHandle>) -> Option<WindowState> {
    let s = settings.lock().unwrap();
    match window_type.as_str() {
        "main" => s.main_window_state.clone(),
        "orb" => s.orb_window_state.clone(),
        _ => None,
    }
}
```

**Step 3: 在 lib.rs 注册新命令**

```rust
.invoke_handler(tauri::generate_handler![
    // ... existing commands ...
    save_window_state,
    load_window_state,
])
```

**Step 4: 提交**
```bash
git add auto-heart/src-tauri/src/settings.rs auto-heart/src-tauri/src/commands.rs auto-heart/src-tauri/src/lib.rs
git commit -m "feat(window): add window state cache commands"
```

---

### Task 5: 前端主窗口位置缓存

**Files:**
- Modify: `auto-heart/src/pages/MainWindow.tsx`

**Step 1: 添加位置保存逻辑**

```typescript
useEffect(() => {
  const savePosition = async () => {
    try {
      const pos = await getCurrentWindow().outerPosition();
      const size = await getCurrentWindow().outerSize();
      await invoke('save_window_state', {
        windowType: 'main',
        state: {
          x: pos.x,
          y: pos.y,
          width: size.width,
          height: size.height,
          isMaximized: await getCurrentWindow().isMaximized(),
        }
      });
    } catch {}
  };

  const handleClose = () => {
    savePosition();
  };

  // 监听窗口关闭事件
  getCurrentWindow().onCloseRequested(handleClose);

  return () => {};
}, []);
```

**Step 2: 提交**
```bash
git add auto-heart/src/pages/MainWindow.tsx
git commit -m "feat(window): save main window position on close"
```

---

## Phase 3: 对话功能

### Task 6: 创建 conversation.rs

**Files:**
- Create: `auto-heart/src-tauri/src/conversation.rs`

**Step 1: 创建对话数据结构和 CRUD 函数**

```rust
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub role: String,  // "user" | "assistant"
    pub content: String,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub messages: Vec<Message>,
}

impl Conversation {
    pub fn new(first_message: &str) -> Self {
        let now = chrono::Local::now().to_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            title: first_message.chars().take(20).collect(),
            created_at: now.clone(),
            updated_at: now,
            messages: vec![],
        }
    }
}

pub fn get_conversations_dir(data_dir: &PathBuf) -> PathBuf {
    let dir = data_dir.join("conversations");
    std::fs::create_dir_all(&dir).ok();
    dir
}

pub fn list_conversations(data_dir: &PathBuf) -> Vec<Conversation> {
    let dir = get_conversations_dir(data_dir);
    let mut conversations = vec![];

    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            if entry.path().extension().map_or(false, |e| e == "json") {
                if let Ok(content) = std::fs::read_to_string(entry.path()) {
                    if let Ok(conv) = serde_json::from_str::<Conversation>(&content) {
                        conversations.push(conv);
                    }
                }
            }
        }
    }

    conversations.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    conversations
}

pub fn save_conversation(data_dir: &PathBuf, conv: &Conversation) -> Result<(), String> {
    let path = get_conversations_dir(data_dir).join(format!("{}.json", conv.id));
    let content = serde_json::to_string_pretty(conv).map_err(|e| e.to_string())?;
    std::fs::write(path, content).map_err(|e| e.to_string())
}

pub fn get_conversation(data_dir: &PathBuf, id: &str) -> Option<Conversation> {
    let path = get_conversations_dir(data_dir).join(format!("{}.json", id));
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn delete_conversation(data_dir: &PathBuf, id: &str) -> Result<(), String> {
    let path = get_conversations_dir(data_dir).join(format!("{}.json", id));
    std::fs::remove_file(path).map_err(|e| e.to_string())
}
```

**Step 2: 提交**
```bash
git add auto-heart/src-tauri/src/conversation.rs
git commit -m "feat(chat): add conversation data structures and CRUD"
```

---

### Task 7: 新增对话 Tauri 命令

**Files:**
- Modify: `auto-heart/src-tauri/src/commands.rs`
- Modify: `auto-heart/src-tauri/src/lib.rs`

**Step 1: 在 commands.rs 添加对话命令**

```rust
#[tauri::command]
pub fn get_conversations(app: AppHandle) -> Vec<Conversation> {
    let data_dir = app.path().app_data_dir().unwrap_or_default();
    conversation::list_conversations(&data_dir)
}

#[tauri::command]
pub fn get_conversation(id: String, app: AppHandle) -> Option<Conversation> {
    let data_dir = app.path().app_data_dir().unwrap_or_default();
    conversation::get_conversation(&data_dir, &id)
}

#[tauri::command]
pub fn create_conversation(first_message: String, app: AppHandle) -> Result<Conversation, String> {
    let data_dir = app.path().app_data_dir().unwrap_or_default();
    let conv = Conversation::new(&first_message);
    conversation::save_conversation(&data_dir, &conv)?;
    Ok(conv)
}

#[tauri::command]
pub fn delete_conversation(id: String, app: AppHandle) -> Result<(), String> {
    let data_dir = app.path().app_data_dir().unwrap_or_default();
    conversation::delete_conversation(&data_dir, &id)
}

#[tauri::command]
pub async fn send_message(
    session_id: String,
    content: String,
    app: AppHandle,
    settings: State<'_, SettingsHandle>,
) -> Result<Message, String> {
    let data_dir = app.path().app_data_dir().unwrap_or_default();

    // 获取或创建会话
    let mut conv = conversation::get_conversation(&data_dir, &session_id)
        .unwrap_or_else(|| Conversation::new(&content));

    // 添加用户消息
    let user_msg = Message {
        id: Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: content.clone(),
        timestamp: chrono::Local::now().to_rfc3339(),
    };
    conv.messages.push(user_msg.clone());
    conv.updated_at = chrono::Local::now().to_rfc3339();
    conversation::save_conversation(&data_dir, &conv)?;

    // 调用 LLM
    let settings_snap = settings.lock().unwrap().clone();
    let model_config = crate::model_router::build_model_config(
        &settings_snap.chat_model,
        &settings_snap.chat_model_name,
        &settings_snap,
    ).ok_or("Chat model not configured")?;

    let messages: Vec<OaiMessage> = conv.messages.iter().map(|m| {
        OaiMessage { role: m.role.clone(), content: m.content.clone() }
    }).collect();

    // 构建 prompt
    let response = call_chat_model(&model_config, &messages).await?;

    // 添加助手消息
    let assistant_msg = Message {
        id: Uuid::new_v4().to_string(),
        role: "assistant".to_string(),
        content: response.clone(),
        timestamp: chrono::Local::now().to_rfc3339(),
    };

    // 更新会话
    conv.messages.push(assistant_msg.clone());
    conv.updated_at = chrono::Local::now().to_rfc3339();
    conversation::save_conversation(&data_dir, &conv)?;

    // 检查是否包含意图（任务相关关键词）
    if contains_intent_keywords(&content) {
        let _ = parse_intent_from_chat_internal(&data_dir, &content, &settings_snap);
    }

    Ok(assistant_msg)
}
```

**Step 2: 添加意图检测和解析辅助函数**

```rust
fn contains_intent_keywords(content: &str) -> bool {
    let keywords = ["今天", "要做", "待办", "计划", "任务", "完成", "开始"];
    keywords.iter().any(|k| content.contains(k))
}

fn parse_intent_from_chat_internal(data_dir: &PathBuf, content: &str, settings: &AppSettings) -> Option<()> {
    let prompt = format!(
        "用户消息：{}\n\n请解析为任务列表，输出 JSON 数组：\n[{{\"time\":\"HH:MM\",\"task\":\"任务描述\",\"tag\":\"关联模块\",\"status\":\"pending\"}}]",
        content
    );

    let config = model_router::build_model_config(&settings.middle_model, &settings.middle_model_name, settings)?;
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().ok()?;
    let router = ModelRouter::new();

    let response = rt.block_on(router.call_with_config(&config, &prompt, Some("你是 Auto-Heart，解析用户任务。"))).ok()?;

    let json_str = extract_json_array(&response);
    if let Ok(tasks) = serde_json::from_str::<serde_json::Value>(json_str) {
        let db = app_data_dir.join("auto_heart.db");
        // 写入 intent_history 表
        // ... (复用现有逻辑)
    }

    Some(())
}
```

**Step 3: 在 lib.rs 注册新命令**

**Step 4: 提交**
```bash
git add auto-heart/src-tauri/src/commands.rs auto-heart/src-tauri/src/lib.rs
git commit -m "feat(chat): add conversation Tauri commands"
```

---

### Task 8: 前端对话 UI

**Files:**
- Modify: `auto-heart/src/pages/ConversationTab.tsx`

**Step 1: 添加对话列表和消息显示**

```typescript
const [conversations, setConversations] = useState<Conversation[]>([]);
const [currentConv, setCurrentConv] = useState<Conversation | null>(null);
const [input, setInput] = useState('');

useEffect(() => {
  loadConversations();
}, []);

const loadConversations = async () => {
  try {
    const list = await invoke<Conversation[]>('get_conversations');
    setConversations(list);
  } catch {}
};

const handleSend = async () => {
  if (!input.trim() || !currentConv) return;
  try {
    const msg = await invoke<Message>('send_message', {
      sessionId: currentConv.id,
      content: input,
    });
    setCurrentConv(prev => prev ? {
      ...prev,
      messages: [...prev.messages, msg],
    } : null);
    setInput('');
    loadConversations(); // 刷新列表
  } catch {}
};
```

**Step 2: 提交**
```bash
git add auto-heart/src/pages/ConversationTab.tsx
git commit -m "feat(chat): add conversation UI"
```

---

### Task 9: 小球点击触发对话 Tab

**Files:**
- Modify: `auto-heart/src/App.tsx`
- Modify: `auto-heart/src/pages/MainWindow.tsx`

**Step 1: 在 App.tsx 中添加 URL 参数检测**

```typescript
// 检测 URL 参数 view=chat
const urlParams = new URLSearchParams(window.location.search);
const initialView = urlParams.get('view') || 'today';
```

**Step 2: 修改 MainWindow 接收初始 Tab 参数**

```typescript
interface MainWindowProps {
  initialTab?: 'today' | 'semantic' | 'conversation' | 'settings';
}
```

**Step 3: 小球点击时传递 view 参数**

```typescript
// open_main_window 时传递 view=chat
await invoke('open_main_window_with_view', { view: 'conversation' });
```

**Step 4: 添加新命令**

```rust
#[tauri::command]
async fn open_main_window_with_view(app: AppHandle, view: String) -> Result<(), String> {
    // 打开窗口并聚焦到指定 Tab
    open_main_window(app.clone()).await?;
    if let Some(window) = app.get_webview_window("main") {
        window.emit("navigate_to", view).map_err(|e| e.to_string())?;
    }
    Ok(())
}
```

**Step 5: 提交**
```bash
git add auto-heart/src/App.tsx auto-heart/src/pages/MainWindow.tsx auto-heart/src-tauri/src/lib.rs auto-heart/src-tauri/src/commands.rs
git commit -m "feat(chat): orb click opens conversation tab"
```

---

## Phase 4: 监听目录优化

### Task 10: 自动检测开发目录

**Files:**
- Modify: `auto-heart/src-tauri/src/settings.rs`
- Modify: `auto-heart/src-tauri/src/heartbeat.rs`

**Step 1: 添加自动检测函数**

```rust
pub fn auto_detect_watch_paths() -> Vec<String> {
    let mut paths = vec![];

    #[cfg(target_os = "windows")]
    {
        if let Ok(user_dir) = std::env::var("USERPROFILE") {
            let base = PathBuf::from(user_dir);
            for name in &["Documents", "Desktop", "Code", "Projects"] {
                let path = base.join(name);
                if path.exists() {
                    paths.push(path.to_string_lossy().to_string());
                }
            }
        }
    }

    paths
}
```

**Step 2: 修改 lib.rs 启动逻辑**

```rust
let mut watch_paths: Vec<PathBuf> = if app_settings.watch_paths.is_empty() {
    auto_detect_watch_paths()
} else {
    app_settings.watch_paths.iter().map(PathBuf::from).collect()
};
```

**Step 3: 提交**
```bash
git add auto-heart/src-tauri/src/settings.rs auto-heart/src-tauri/src/lib.rs
git commit -m "feat(watch): auto-detect common dev directories"
```

---

## 实现顺序

1. Task 1-3: 设置持久化（JSON 文件）
2. Task 4-5: 窗口缓存
3. Task 6-9: 对话功能
4. Task 10: 监听目录优化
