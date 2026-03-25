# Auto-Heart Orb + Main Window Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 创建双窗口 Tauri 应用：Orb 小球（右侧边缘，Canvas 渲染，三态动画）+ 主窗口（点击 Orb 展开，模型配置）

**Architecture:**
- Tauri 2.x 多窗口：Orb 窗口（透明、alwaysOnTop）+ 主窗口（普通窗口）
- 前端：React + TypeScript + Canvas API
- 后端：Rust 心跳调度 + 本地存储
- 窗口通信：Tauri event system

**Tech Stack:** Tauri 2.x, React 18, TypeScript, Canvas API, tauri-plugin-store (本地存储)

---

## Task 1: 初始化 Tauri 项目

**Goal:** 创建 Tauri + React + TypeScript 项目脚手架

**Files:**
- Create: `auto-heart/` (entire project structure)
- Modify: N/A

**Step 1: 创建 Tauri 项目**

Run: `cd D:/Agent/Auto-Heart && npm create tauri-app@latest auto-heart -- --template react-ts --manager npm`

Expected: 项目创建成功，结构包含 `src/` 和 `src-tauri/`

**Step 2: 验证项目结构**

```
auto-heart/
├── src/                    # React 前端
├── src-tauri/              # Rust 后端
│   ├── src/
│   │   ├── main.rs
│   │   └── lib.rs
│   ├── Cargo.toml
│   └── tauri.conf.json
├── package.json
└── tsconfig.json
```

**Step 3: 启动开发服务器验证**

Run: `cd auto-heart && npm run tauri dev`
Expected: Tauri 窗口打开，无报错

**Step 4: 提交**

```bash
git add .
git commit -m "chore: scaffold Tauri + React + TypeScript project"
```

---

## Task 2: 配置 Orb 窗口

**Goal:** 配置 tauri.conf.json 创建 Orb 专用窗口（透明、alwaysOnTop、右侧边缘）

**Files:**
- Modify: `auto-heart/src-tauri/tauri.conf.json`
- Modify: `auto-heart/src-tauri/src/lib.rs`

**Step 1: 配置 Orb 窗口参数**

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "auto-heart",
  "version": "0.1.0",
  "identifier": "com.auto-heart.app",
  "build": {
    "beforeDevCommand": "npm run dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "npm run build",
    "frontendDist": "../dist",
    "devtools": true
  },
  "app": {
    "withGlobalTauri": true,
    "windows": [
      {
        "label": "orb",
        "title": "Auto-Heart Orb",
        "width": 120,
        "height": 120,
        "transparent": true,
        "decorations": false,
        "alwaysOnTop": true,
        "resizable": false,
        "skipTaskbar": true,
        "visible": true,
        "url": "index.html"
      }
    ],
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  }
}
```

**Step 2: 添加窗口管理依赖到 Cargo.toml**

```toml
[dependencies]
tauri = { version = "2", features = ["devtools"] }
tauri-plugin-store = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

Run: `cd auto-heart/src-tauri && cargo check`

Expected: 无编译错误

**Step 3: 在 lib.rs 中添加多窗口创建命令**

```rust
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

#[tauri::command]
fn open_main_window(app: AppHandle) -> Result<(), String> {
    // 检查主窗口是否已存在
    if app.get_webview_window("main").is_some() {
        // 如果已存在，聚焦并返回
        if let Some(window) = app.get_webview_window("main") {
            window.set_focus().map_err(|e| e.to_string())?;
        }
        return Ok(());
    }

    // 创建主窗口
    WebviewWindowBuilder::new(
        &app,
        "main",
        WebviewUrl::App("index.html".into())
    )
    .title("Auto-Heart")
    .inner_size(800.0, 600.0)
    .center()
    .decorations(true)
    .resizable(true)
    .build()
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .invoke_handler(tauri::generate_handler![open_main_window])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**Step 4: 运行 cargo check 验证**

Run: `cd auto-heart/src-tauri && cargo check`

Expected: 无编译错误

**Step 5: 提交**

```bash
git add -A
git commit -m "feat: configure Orb window with alwaysOnTop and transparency"
```

---

## Task 3: 实现 Orb Canvas 组件

**Goal:** 创建 Canvas 渲染的 Orb 小球，三态动画（沉默/思考/发言）

**Files:**
- Create: `auto-heart/src/components/Orb.tsx`
- Modify: `auto-heart/src/App.tsx`

**Step 1: 创建 Orb.tsx**

```tsx
import { useEffect, useRef, useState, useCallback } from 'react';

// Orb 状态类型
type OrbState = 'idle' | 'thinking' | 'speaking';

interface OrbProps {
  onClick: () => void;
  state?: OrbState;
}

// Orb 配置常量
const ORB_CONFIG = {
  // 尺寸
  coreRadius: 24,
  ringWidth: 36,
  innerRadius: 10,

  // 颜色
  coreColor: '#EEEDFE',
  innerColor: '#534AB7',
  ringColor: '#AFA9EC',

  // 动画周期 (ms)
  breathePeriod: 3200,
  pulseRingPeriod: 3200,
  thinkingPeriod: 2000,
  bubblePeriod: 4000,
};

export default function Orb({ onClick, state = 'idle' }: OrbProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const animationRef = useRef<number>(0);
  const startTimeRef = useRef<number>(0);

  // 计算呼吸动画的相位值 (0-1)
  const getBreathPhase = useCallback((elapsed: number, period: number) => {
    return (Math.sin((elapsed / period) * Math.PI * 2 - Math.PI / 2) + 1) / 2;
  }, []);

  // 绘制函数
  const draw = useCallback((timestamp: number) => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const elapsed = timestamp - startTimeRef.current;
    const { coreRadius, innerRadius, coreColor, innerColor, ringColor } = ORB_CONFIG;

    // 清除画布
    ctx.clearRect(0, 0, canvas.width, canvas.height);

    // 中心点
    const centerX = canvas.width / 2;
    const centerY = canvas.height / 2;

    if (state === 'idle') {
      // 沉默态：呼吸动画
      const phase = getBreathPhase(elapsed, ORB_CONFIG.breathePeriod);
      const scale = 1 + phase * 0.22;  // 1.0 - 1.22
      const alpha = 0.55 + phase * 0.45;  // 0.55 - 1.0

      // 绘制脉冲环
      ctx.save();
      ctx.globalAlpha = alpha * 0.4;
      ctx.strokeStyle = ringColor;
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      ctx.arc(centerX, centerY, coreRadius * scale * (1 + phase * 0.3), 0, Math.PI * 2);
      ctx.stroke();
      ctx.restore();

      // 绘制核心
      ctx.save();
      ctx.globalAlpha = alpha;
      ctx.shadowBlur = 20;
      ctx.shadowColor = ringColor;
      ctx.fillStyle = coreColor;
      ctx.beginPath();
      ctx.arc(centerX, centerY, coreRadius * scale, 0, Math.PI * 2);
      ctx.fill();

      // 绘制内芯
      ctx.shadowBlur = 10;
      ctx.shadowColor = innerColor;
      ctx.fillStyle = innerColor;
      ctx.beginPath();
      ctx.arc(centerX, centerY, innerRadius * scale, 0, Math.PI * 2);
      ctx.fill();
      ctx.restore();

    } else if (state === 'thinking') {
      // 思考态：轻微旋转抖动
      const cycleElapsed = elapsed % ORB_CONFIG.thinkingPeriod;
      const phase = getBreathPhase(cycleElapsed, ORB_CONFIG.thinkingPeriod);
      const rotation = Math.sin(phase * Math.PI * 2) * 0.1;  // ±5度
      const scale = 1 + Math.abs(Math.sin(phase * Math.PI * 2)) * 0.1;
      const alpha = 0.6 + phase * 0.4;

      ctx.save();
      ctx.globalAlpha = alpha;
      ctx.translate(centerX, centerY);
      ctx.rotate(rotation);
      ctx.translate(-centerX, -centerY);

      ctx.shadowBlur = 25;
      ctx.shadowColor = innerColor;
      ctx.fillStyle = coreColor;
      ctx.beginPath();
      ctx.arc(centerX, centerY, coreRadius * 1.1 * scale, 0, Math.PI * 2);
      ctx.fill();

      ctx.shadowBlur = 15;
      ctx.fillStyle = innerColor;
      ctx.beginPath();
      ctx.arc(centerX, centerY, innerRadius * 1.1, 0, Math.PI * 2);
      ctx.fill();
      ctx.restore();

    } else if (state === 'speaking') {
      // 发言态：气泡已由父组件处理，这里只显示稍大的核心
      ctx.save();
      ctx.shadowBlur = 30;
      ctx.shadowColor = innerColor;
      ctx.fillStyle = coreColor;
      ctx.beginPath();
      ctx.arc(centerX, centerY, coreRadius * 1.15, 0, Math.PI * 2);
      ctx.fill();

      ctx.shadowBlur = 20;
      ctx.fillStyle = innerColor;
      ctx.beginPath();
      ctx.arc(centerX, centerY, innerRadius * 1.2, 0, Math.PI * 2);
      ctx.fill();
      ctx.restore();
    }

    animationRef.current = requestAnimationFrame(draw);
  }, [state, getBreathPhase]);

  // 启动动画循环
  useEffect(() => {
    startTimeRef.current = performance.now();
    animationRef.current = requestAnimationFrame(draw);

    return () => {
      cancelAnimationFrame(animationRef.current);
    };
  }, [draw]);

  return (
    <canvas
      ref={canvasRef}
      width={120}
      height={120}
      onClick={onClick}
      style={{
        cursor: 'pointer',
        display: 'block',
      }}
    />
  );
}
```

**Step 2: 修改 App.tsx 使用 Orb 组件**

```tsx
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import Orb from './components/Orb';
import './App.css';

function App() {
  const [orbState, setOrbState] = useState<'idle' | 'thinking' | 'speaking'>('idle');

  const handleOrbClick = async () => {
    try {
      await invoke('open_main_window');
    } catch (err) {
      console.error('Failed to open main window:', err);
    }
  };

  // 模拟心跳切换到 thinking 状态
  useEffect(() => {
    const interval = setInterval(() => {
      setOrbState('thinking');
      setTimeout(() => setOrbState('idle'), 2000);
    }, 10000); // 每 10 秒切换一次

    return () => clearInterval(interval);
  }, []);

  return (
    <div
      style={{
        position: 'fixed',
        right: 20,
        bottom: 20,
        width: 120,
        height: 120,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        background: 'transparent',
      }}
    >
      <Orb onClick={handleOrbClick} state={orbState} />
    </div>
  );
}

export default App;
```

**Step 3: 验证渲染**

Run: `cd auto-heart && npm run tauri dev`

Expected:
- 右侧底部显示 Orb 小球
- 沉默态有呼吸动画
- 每 10 秒切换到思考态
- 点击打开主窗口（首次点击会创建）

**Step 4: 提交**

```bash
git add src/components/Orb.tsx src/App.tsx
git commit -m "feat: add Orb canvas component with three states animation"
```

---

## Task 4: 实现气泡组件

**Goal:** 发言态的气泡从 Orb 浮现

**Files:**
- Create: `auto-heart/src/components/SpeechBubble.tsx`
- Modify: `auto-heart/src/App.tsx`

**Step 1: 创建 SpeechBubble.tsx**

```tsx
import { useEffect, useState } from 'react';

interface SpeechBubbleProps {
  message: {
    title: string;
    content: string;
  };
  onDismiss: () => void;
}

export default function SpeechBubble({ message, onDismiss }: SpeechBubbleProps) {
  const [visible, setVisible] = useState(false);

  useEffect(() => {
    // 延迟显示，让气泡从下方升起
    const showTimer = setTimeout(() => setVisible(true), 50);

    // 4 秒后自动消失
    const dismissTimer = setTimeout(() => {
      setVisible(false);
      setTimeout(onDismiss, 300); // 等待动画完成
    }, 4000);

    return () => {
      clearTimeout(showTimer);
      clearTimeout(dismissTimer);
    };
  }, [onDismiss]);

  return (
    <div
      style={{
        position: 'absolute',
        right: 130,
        bottom: 0,
        opacity: visible ? 1 : 0,
        transform: visible ? 'translateY(0)' : 'translateY(10px)',
        transition: 'all 0.3s ease-out',
        pointerEvents: visible ? 'auto' : 'none',
      }}
    >
      <div
        style={{
          background: 'rgba(20, 20, 30, 0.95)',
          border: '0.5px solid #AFA9EC',
          borderRadius: '12px 12px 0 12px',
          padding: '10px 14px',
          maxWidth: 220,
          backdropFilter: 'blur(8px)',
        }}
      >
        <div
          style={{
            fontSize: 11,
            color: '#534AB7',
            marginBottom: 4,
            fontWeight: 500,
          }}
        >
          {message.title}
        </div>
        <div
          style={{
            fontSize: 12,
            color: '#CCCCCC',
            lineHeight: 1.5,
          }}
        >
          {message.content}
        </div>
        <div
          style={{
            display: 'flex',
            gap: 6,
            marginTop: 8,
          }}
        >
          <button
            style={{
              fontSize: 11,
              padding: '3px 8px',
              borderRadius: 5,
              cursor: 'pointer',
              background: '#EEEDFE',
              border: '0.5px solid #AFA9EC',
              color: '#534AB7',
            }}
          >
            帮我改
          </button>
          <button
            style={{
              fontSize: 11,
              padding: '3px 8px',
              borderRadius: 5,
              cursor: 'pointer',
              background: 'transparent',
              border: '0.5px solid #666',
              color: '#999',
            }}
            onClick={() => {
              setVisible(false);
              setTimeout(onDismiss, 300);
            }}
          >
            忽略
          </button>
        </div>
      </div>
    </div>
  );
}
```

**Step 2: 更新 App.tsx 集成气泡**

```tsx
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import Orb from './components/Orb';
import SpeechBubble from './components/SpeechBubble';
import './App.css';

function App() {
  const [orbState, setOrbState] = useState<'idle' | 'thinking' | 'speaking'>('idle');
  const [bubbleMessage, setBubbleMessage] = useState<{title: string; content: string} | null>(null);

  const handleOrbClick = async () => {
    try {
      await invoke('open_main_window');
    } catch (err) {
      console.error('Failed to open main window:', err);
    }
  };

  const handleBubbleDismiss = () => {
    setBubbleMessage(null);
    setOrbState('idle');
  };

  // 模拟气泡消息（每 30 秒一次）
  useEffect(() => {
    const interval = setInterval(() => {
      setOrbState('speaking');
      setBubbleMessage({
        title: '注意到一个问题',
        content: 'verify() 缺少过期校验，存在安全风险。',
      });
      setTimeout(() => {
        if (!bubbleMessage) {
          setOrbState('idle');
        }
      }, 4000);
    }, 30000);

    return () => clearInterval(interval);
  }, []);

  return (
    <div
      style={{
        position: 'fixed',
        right: 20,
        bottom: 20,
        width: 120,
        height: 120,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        background: 'transparent',
      }}
    >
      <Orb onClick={handleOrbClick} state={orbState} />
      {bubbleMessage && (
        <SpeechBubble message={bubbleMessage} onDismiss={handleBubbleDismiss} />
      )}
    </div>
  );
}

export default App;
```

**Step 3: 验证**

Run: `cd auto-heart && npm run tauri dev`

Expected:
- 气泡从右侧浮现
- 4 秒后自动消失
- 有"帮我改"和"忽略"按钮

**Step 4: 提交**

```bash
git add src/components/SpeechBubble.tsx src/App.tsx
git commit -m "feat: add speech bubble component with auto-dismiss"
```

---

## Task 5: 创建主窗口页面

**Goal:** 主窗口包含今日任务、消息队列、语义地图摘要、模型配置标签页

**Files:**
- Create: `auto-heart/src/pages/MainWindow.tsx`
- Create: `auto-heart/src/pages/TodayTab.tsx`
- Create: `auto-heart/src/pages/SemanticMapTab.tsx`
- Create: `auto-heart/src/pages/SettingsTab.tsx`
- Modify: `auto-heart/src/App.tsx` (Orb 窗口)

**Step 1: 创建 TodayTab.tsx**

```tsx
export default function TodayTab() {
  // 模拟数据
  const tasks = [
    { time: '10:00', task: 'refreshToken 过期校验', tag: 'auth/guard.ts', status: 'pending' },
    { time: '13:00', task: 'dashboard 接口联调', tag: 'UserService', status: 'pending' },
    { time: '14:30', task: '技术评审 — 我来帮你整理', tag: '进行中', status: 'active' },
    { time: '18:00', task: '日报', tag: '我来写', status: 'pending' },
  ];

  const queueMessages = [
    { level: 'warning', text: 'Guard 缺少 tenantId — flow 结束后提醒' },
    { level: 'info', text: 'UserService 可进一步优化 — 日报时附带' },
  ];

  return (
    <div style={{ padding: 16, display: 'flex', flexDirection: 'column', gap: 12 }}>
      {/* 今日意图 */}
      <div>
        <div style={{ fontSize: 11, color: '#666', marginBottom: 6 }}>今日意图</div>
        <div style={{
          background: '#1a1a2e',
          borderRadius: 8,
          padding: '10px 12px',
          fontSize: 12,
          color: '#999',
          lineHeight: 1.8,
        }}>
          完成 refreshToken 过期校验 → 10:00<br/>
          联调 dashboard 接口 → 下午<br/>
          3点技术评审，准备 auth 说明<br/>
          <span style={{ color: '#666' }}>下班前写日报</span>
        </div>
      </div>

      {/* 已解析任务 */}
      <div>
        <div style={{ fontSize: 11, color: '#666', marginBottom: 6 }}>已解析为</div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 5 }}>
          {tasks.map((t, i) => (
            <div
              key={i}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 10,
                padding: '7px 10px',
                background: t.status === 'active' ? '#EEEDFE' : '#1a1a2e',
                borderRadius: 7,
                border: t.status === 'active' ? '0.5px solid #AFA9EC' : 'none',
              }}
            >
              <span style={{ fontSize: 11, fontWeight: 500, color: '#534AB7', minWidth: 36 }}>
                {t.time}
              </span>
              <span style={{ fontSize: 12, color: t.status === 'active' ? '#3C3489' : '#eee', flex: 1 }}>
                {t.task}
              </span>
              <span style={{ fontSize: 10, color: '#666' }}>{t.tag}</span>
            </div>
          ))}
        </div>
      </div>

      {/* 消息队列 */}
      <div>
        <div style={{ fontSize: 11, color: '#666', marginBottom: 6 }}>等待发送的消息</div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 5 }}>
          {queueMessages.map((m, i) => (
            <div
              key={i}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 8,
                padding: '7px 10px',
                background: '#1a1a2e',
                borderRadius: 7,
              }}
            >
              <div
                style={{
                  width: 5,
                  height: 5,
                  borderRadius: '50%',
                  background: m.level === 'warning' ? '#FFB020' : '#666',
                  flexShrink: 0,
                }}
              />
              <span style={{ fontSize: 12, color: m.level === 'warning' ? '#FFB020' : '#666' }}>
                {m.text}
              </span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
```

**Step 2: 创建 SemanticMapTab.tsx**

```tsx
export default function SemanticMapTab() {
  const modules = [
    {
      name: 'auth/guard.ts',
      desc: '权限守卫模块',
      lastUpdate: '2小时前',
      understanding: '负责验证用户身份和访问权限，入口为 verify() 函数',
    },
    {
      name: 'UserService',
      desc: '用户服务',
      lastUpdate: '昨天',
      understanding: '处理用户 CRUD 操作，当前缺少 batch 接口',
    },
    {
      name: 'dashboard/api.ts',
      desc: '仪表盘接口',
      lastUpdate: '3天前',
      understanding: '提供数据统计和可视化接口，使用 REST 风格',
    },
  ];

  return (
    <div style={{ padding: 16, display: 'flex', flexDirection: 'column', gap: 12 }}>
      <div style={{ fontSize: 11, color: '#666', marginBottom: 4 }}>
        项目理解摘要 · 3 个模块
      </div>
      {modules.map((m, i) => (
        <div
          key={i}
          style={{
            background: '#1a1a2e',
            borderRadius: 8,
            padding: 12,
          }}
        >
          <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: 6 }}>
            <span style={{ fontSize: 13, fontWeight: 500, color: '#534AB7' }}>{m.name}</span>
            <span style={{ fontSize: 10, color: '#666' }}>{m.lastUpdate}</span>
          </div>
          <div style={{ fontSize: 11, color: '#666', marginBottom: 4 }}>{m.desc}</div>
          <div style={{ fontSize: 12, color: '#999', lineHeight: 1.5 }}>
            {m.understanding}
          </div>
        </div>
      ))}
    </div>
  );
}
```

**Step 3: 创建 SettingsTab.tsx**

```tsx
import { useState } from 'react';

interface ModelConfig {
  name: string;
  provider: string;
  tokenCost: string;
  enabled: boolean;
}

export default function SettingsTab() {
  const [models, setModels] = useState<ModelConfig[]>([
    { name: '本地规则引擎', provider: '-', tokenCost: '0 token', enabled: true },
    { name: 'Kimi', provider: '用户 Key', tokenCost: '低消耗', enabled: true },
    { name: 'Claude', provider: '用户 Key', tokenCost: '每日几次', enabled: true },
  ]);

  const [activeTab, setActiveTab] = useState<'浅层' | '中层' | '深层'>('浅层');

  return (
    <div style={{ padding: 16 }}>
      {/* 心跳层级选择 */}
      <div style={{ marginBottom: 16 }}>
        <div style={{ fontSize: 11, color: '#666', marginBottom: 8 }}>心跳层级配置</div>
        <div style={{ display: 'flex', gap: 4 }}>
          {(['浅层', '中层', '深层'] as const).map((tab) => (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              style={{
                padding: '6px 16px',
                fontSize: 12,
                borderRadius: 6,
                border: activeTab === tab ? '0.5px solid #534AB7' : '0.5px solid #333',
                background: activeTab === tab ? '#EEEDFE' : 'transparent',
                color: activeTab === tab ? '#534AB7' : '#666',
                cursor: 'pointer',
              }}
            >
              {tab}
            </button>
          ))}
        </div>
      </div>

      {/* 模型配置 */}
      <div style={{ fontSize: 11, color: '#666', marginBottom: 8 }}>模型配置</div>
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 8 }}>
        {models.map((model, i) => (
          <div
            key={i}
            style={{
              background: '#1a1a2e',
              borderRadius: 7,
              padding: '8px 10px',
            }}
          >
            <div style={{ fontSize: 10, color: '#666', marginBottom: 3 }}>{tab}心跳</div>
            <div style={{ fontSize: 12, color: '#999' }}>{model.name}</div>
            <div style={{ fontSize: 10, color: model.enabled ? '#534AB7' : '#666', marginTop: 2 }}>
              {model.tokenCost}
            </div>
          </div>
        ))}
      </div>

      {/* API Key 配置 */}
      <div style={{ marginTop: 20 }}>
        <div style={{ fontSize: 11, color: '#666', marginBottom: 8 }}>API Key 配置</div>
        <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
          <div>
            <label style={{ fontSize: 10, color: '#666', display: 'block', marginBottom: 4 }}>
              Claude API Key
            </label>
            <input
              type="password"
              placeholder="sk-..."
              style={{
                width: '100%',
                padding: '8px 10px',
                fontSize: 12,
                background: '#1a1a2e',
                border: '0.5px solid #333',
                borderRadius: 6,
                color: '#eee',
                boxSizing: 'border-box',
              }}
            />
          </div>
          <div>
            <label style={{ fontSize: 10, color: '#666', display: 'block', marginBottom: 4 }}>
              Kimi API Key
            </label>
            <input
              type="password"
              placeholder="sk-..."
              style={{
                width: '100%',
                padding: '8px 10px',
                fontSize: 12,
                background: '#1a1a2e',
                border: '0.5px solid #333',
                borderRadius: 6,
                color: '#eee',
                boxSizing: 'border-box',
              }}
            />
          </div>
        </div>
      </div>
    </div>
  );
}
```

**Step 4: 创建 MainWindow.tsx**

```tsx
import { useState } from 'react';
import TodayTab from './TodayTab';
import SemanticMapTab from './SemanticMapTab';
import SettingsTab from './SettingsTab';

type Tab = 'today' | 'semantic-map' | 'settings';

export default function MainWindow() {
  const [activeTab, setActiveTab] = useState<Tab>('today');

  const tabs: { id: Tab; label: string }[] = [
    { id: 'today', label: '今天' },
    { id: 'semantic-map', label: '语义地图' },
    { id: 'settings', label: '设置' },
  ];

  return (
    <div
      style={{
        height: '100vh',
        display: 'flex',
        flexDirection: 'column',
        background: '#0a0a14',
        color: '#eee',
        fontFamily: 'system-ui, -apple-system, sans-serif',
      }}
    >
      {/* 标题栏 */}
      <div
        style={{
          padding: '10px 16px',
          borderBottom: '0.5px solid #222',
          display: 'flex',
          alignItems: 'center',
          gap: 10,
          background: '#111',
        }}
      >
        <div
          style={{
            width: 20,
            height: 20,
            borderRadius: '50%',
            background: '#EEEDFE',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
          }}
        >
          <div
            style={{
              width: 8,
              height: 8,
              borderRadius: '50%',
              background: '#534AB7',
            }}
          />
        </div>
        <span style={{ fontSize: 13, fontWeight: 500 }}>Auto-Heart</span>
        <span
          style={{
            marginLeft: 'auto',
            fontSize: 11,
            color: '#4CAF50',
            display: 'flex',
            alignItems: 'center',
            gap: 4,
          }}
        >
          <span
            style={{
              width: 5,
              height: 5,
              borderRadius: '50%',
              background: 'currentColor',
              display: 'inline-block',
            }}
          />
          活跃
        </span>
      </div>

      {/* 标签页 */}
      <div
        style={{
          display: 'flex',
          borderBottom: '0.5px solid #222',
        }}
      >
        {tabs.map((tab) => (
          <div
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            style={{
              padding: '8px 16px',
              fontSize: 12,
              color: activeTab === tab.id ? '#534AB7' : '#666',
              borderBottom: activeTab === tab.id ? '2px solid #534AB7' : '2px solid transparent',
              cursor: 'pointer',
            }}
          >
            {tab.label}
          </div>
        ))}
      </div>

      {/* 内容区 */}
      <div style={{ flex: 1, overflow: 'auto' }}>
        {activeTab === 'today' && <TodayTab />}
        {activeTab === 'semantic-map' && <SemanticMapTab />}
        {activeTab === 'settings' && <SettingsTab />}
      </div>
    </div>
  );
}
```

**Step 5: 创建主窗口入口文件**

Create: `auto-heart/src/main-window.tsx`

```tsx
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import MainWindow from './pages/MainWindow';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <MainWindow />
  </StrictMode>
);
```

**Step 6: 修改 lib.rs 支持多入口**

```rust
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

#[tauri::command]
fn open_main_window(app: AppHandle) -> Result<(), String> {
    if app.get_webview_window("main").is_some() {
        if let Some(window) = app.get_webview_window("main") {
            window.set_focus().map_err(|e| e.to_string())?;
        }
        return Ok(());
    }

    WebviewWindowBuilder::new(
        &app,
        "main",
        WebviewUrl::App("main-window.html".into())
    )
    .title("Auto-Heart")
    .inner_size(800.0, 600.0)
    .center()
    .decorations(true)
    .resizable(true)
    .build()
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .invoke_handler(tauri::generate_handler![open_main_window])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**Step 7: 添加第二个 HTML 入口**

Create: `auto-heart/dist/main-window.html`

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Auto-Heart</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main-window.tsx"></script>
  </body>
</html>
```

**Step 8: 验证**

Run: `cd auto-heart && npm run tauri dev`
- 点击 Orb 小球
- 主窗口打开，包含三个标签页
- 模型配置可用

**Step 9: 提交**

```bash
git add src/pages/ src/main-window.tsx dist/main-window.html src-tauri/src/lib.rs
git commit -m "feat: add main window with today/semantic-map/settings tabs"
```

---

## Task 6: 配置系统托盘

**Goal:** 应用最小化到系统托盘，托盘图标右键菜单

**Files:**
- Modify: `auto-heart/src-tauri/tauri.conf.json`
- Modify: `auto-heart/src-tauri/src/lib.rs`

**Step 1: 添加托盘图标**

Run: `cd auto-heart/src-tauri && npm run tauri icon -- ../files/icon.png` (如果有 icon.png)

Or copy existing icon to tray icon location

**Step 2: 修改 lib.rs 添加托盘功能**

```rust
use tauri::{
    AppHandle, Manager,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    WebviewUrl, WebviewWindowBuilder,
};

#[tauri::command]
fn open_main_window(app: AppHandle) -> Result<(), String> {
    if app.get_webview_window("main").is_some() {
        if let Some(window) = app.get_webview_window("main") {
            window.set_focus().map_err(|e| e.to_string())?;
        }
        return Ok(());
    }

    WebviewWindowBuilder::new(
        &app,
        "main",
        WebviewUrl::App("main-window.html".into())
    )
    .title("Auto-Heart")
    .inner_size(800.0, 600.0)
    .center()
    .decorations(true)
    .resizable(true)
    .build()
    .map_err(|e| e.to_string())?;

    Ok(())
}

fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show_item = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

    let _tray = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("Auto-Heart")
        .on_menu_event(|app, event| {
            match event.id.as_ref() {
                "show" => {
                    let _ = open_main_window(app.clone());
                }
                "quit" => {
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                let _ = open_main_window(app.clone());
            }
        })
        .build(app)?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .setup(|app| {
            setup_tray(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![open_main_window])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**Step 3: 运行验证**

Run: `cd auto-heart/src-tauri && cargo check`

**Step 4: 提交**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: add system tray with menu"
```

---

## Task 7: 实现 Rust 端心跳调度器（简化版）

**Goal:** 浅层心跳每 30 秒触发一次，中层心跳每 10 分钟

**Files:**
- Create: `auto-heart/src-tauri/src/heartbeat.rs`
- Modify: `auto-heart/src-tauri/src/lib.rs`

**Step 1: 创建 heartbeat.rs**

```rust
use tauri::{AppHandle, Emitter};
use std::time::Duration;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

#[derive(Clone)]
pub struct HeartbeatState {
    pub shallow_active: Arc<AtomicBool>,
    pub middle_active: Arc<AtomicBool>,
}

impl HeartbeatState {
    pub fn new() -> Self {
        Self {
            shallow_active: Arc::new(AtomicBool::new(true)),
            middle_active: Arc::new(AtomicBool::new(true)),
        }
    }
}

pub fn start_shallow_heartbeat(app: AppHandle) {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(30));
            if let Err(e) = app.emit("heartbeat:shallow", ()) {
                eprintln!("Failed to emit shallow heartbeat: {}", e);
            }
        }
    });
}

pub fn start_middle_heartbeat(app: AppHandle) {
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(600)); // 10 分钟
            if let Err(e) = app.emit("heartbeat:middle", ()) {
                eprintln!("Failed to emit middle heartbeat: {}", e);
            }
        }
    });
}
```

**Step 2: 更新 lib.rs**

```rust
mod heartbeat;

use heartbeat::{start_shallow_heartbeat, start_middle_heartbeat};

#[tauri::command]
fn open_main_window(app: AppHandle) -> Result<(), String> {
    // ... 同上
}

fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // ... 同上
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .setup(|app| {
            setup_tray(app.handle())?;

            // 启动心跳调度器
            start_shallow_heartbeat(app.handle().clone());
            start_middle_heartbeat(app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![open_main_window])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**Step 3: 前端监听心跳事件**

Modify: `auto-heart/src/components/Orb.tsx`

```tsx
import { useEffect, useRef, useState, useCallback } from 'react';

type OrbState = 'idle' | 'thinking' | 'speaking';

interface OrbProps {
  onClick: () => void;
  state?: OrbState;
}

const ORB_CONFIG = {
  coreRadius: 24,
  ringWidth: 36,
  innerRadius: 10,
  coreColor: '#EEEDFE',
  innerColor: '#534AB7',
  ringColor: '#AFA9EC',
  breathePeriod: 3200,
  thinkingPeriod: 2000,
};

export default function Orb({ onClick, state = 'idle' }: OrbProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const animationRef = useRef<number>(0);
  const startTimeRef = useRef<number>(0);

  // 监听 Rust 心跳事件
  useEffect(() => {
    let unlistenShallow: (() => void) | undefined;
    let unlistenMiddle: (() => void) | undefined;

    const setup = async () => {
      const { listen } = await import('@tauri-apps/api/event');

      unlistenShallow = await listen('heartbeat:shallow', () => {
        console.log('Shallow heartbeat received');
      });

      unlistenMiddle = await listen('heartbeat:middle', () => {
        console.log('Middle heartbeat received');
      });
    };

    setup();

    return () => {
      unlistenShallow?.();
      unlistenMiddle?.();
    };
  }, []);

  // ... 其余代码保持不变
}
```

**Step 4: 验证**

Run: `cargo check && npm run tauri dev`

**Step 5: 提交**

```bash
git add src-tauri/src/heartbeat.rs src-tauri/src/lib.rs src/components/Orb.tsx
git commit -m "feat: implement heartbeat scheduler with shallow and middle layers"
```

---

## Task 8: 本地存储配置

**Goal:** 使用 tauri-plugin-store 保存用户设置和 API Keys

**Files:**
- Modify: `auto-heart/src-tauri/src/lib.rs`
- Create: `auto-heart/src/hooks/useSettings.ts`

**Step 1: 创建 useSettings hook**

```tsx
import { useState, useEffect } from 'react';
import { Store } from '@tauri-apps/plugin-store';

interface Settings {
  claudeApiKey: string;
  kimiApiKey: string;
  breathSpeed: 'slow' | 'normal' | 'fast';
  silentMode: boolean;
}

const DEFAULT_SETTINGS: Settings = {
  claudeApiKey: '',
  kimiApiKey: '',
  breathSpeed: 'normal',
  silentMode: false,
};

export function useSettings() {
  const [settings, setSettings] = useState<Settings>(DEFAULT_SETTINGS);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const loadSettings = async () => {
      try {
        const store = await Store.load('settings.json');
        const saved = await store.get<Settings>('settings');
        if (saved) {
          setSettings({ ...DEFAULT_SETTINGS, ...saved });
        }
      } catch (err) {
        console.error('Failed to load settings:', err);
      } finally {
        setLoading(false);
      }
    };

    loadSettings();
  }, []);

  const updateSettings = async (newSettings: Partial<Settings>) => {
    const updated = { ...settings, ...newSettings };
    setSettings(updated);

    try {
      const store = await Store.load('settings.json');
      await store.set('settings', updated);
      await store.save();
    } catch (err) {
      console.error('Failed to save settings:', err);
    }
  };

  return { settings, updateSettings, loading };
}
```

**Step 2: 验证**

Run: `npm run tauri dev`

**Step 3: 提交**

```bash
git add src/hooks/useSettings.ts
git commit -m "feat: add settings persistence with tauri-plugin-store"
```

---

## Task 9: 构建验证

**Goal:** 验证 `npm run tauri build` 成功

**Step 1: 运行构建**

Run: `cd auto-heart && npm run tauri build`

Expected: 构建成功，生成 `src-tauri/target/release/` 下的可执行文件

**Step 2: 测试打包后的应用**

手动测试:
1. Orb 小球显示在右侧
2. 点击打开主窗口
3. 三个标签页切换正常
4. 系统托盘存在

**Step 3: 提交**

```bash
git add -A
git commit -m "feat: complete Orb + Main Window implementation"
```

---

**Plan complete.**