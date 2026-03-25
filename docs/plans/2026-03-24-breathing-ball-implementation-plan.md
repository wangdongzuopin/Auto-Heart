# Auto-Heart 呼吸小球实现计划

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** 在屏幕左下角创建一个发光粒子云团，沉默时轻柔呼吸，点击跳一下，拖拽可移动，事件触发时气泡浮现。

**Architecture:** Tauri 桌面应用，前端 React + Canvas 渲染粒子系统，后端 Rust 处理感知层和心跳调度。粒子动画在前端 Canvas/CanvasKit 实现，状态机管理三种状态切换，数据本地存储。

**Tech Stack:** Tauri 2.x, React 18, TypeScript, Canvas API, 本地 JSON 存储

---

## Task 1: 初始化 Tauri 项目

**Goal:** 创建 Tauri + React + TypeScript 项目脚手架

**Steps:**
1. 运行 `npm create tauri-app@latest auto-heart -- --template react-ts --manager npm`
2. 验证项目结构包含 src/ 和 src-tauri/
3. 提交: `chore: scaffold Tauri project with React+TypeScript`

---

## Task 2: 创建粒子云组件

**Goal:** 在屏幕左下角渲染发光粒子云团

**Files:**
- Create: `src/components/ParticleCloud.tsx`
- Modify: `src/App.tsx`

**Steps:**
1. 创建 ParticleCloud.tsx，Canvas 渲染 12 个粒子
2. 粒子参数：BASE_RADIUS=4, SPREAD_RADIUS=30, PARTICLE_COUNT=12
3. 验证粒子云渲染在左下角
4. 提交: `feat: add particle cloud component`

---

## Task 3: 实现沉默呼吸状态

**Goal:** 粒子云持续明暗交替，模拟呼吸

**Files:**
- Create: `src/hooks/useBreathingAnimation.ts`
- Modify: `src/components/ParticleCloud.tsx`

**Steps:**
1. 编写 useBreathingAnimation hook，返回呼吸相位 0-1
2. alpha 在 0.3-0.8 之间平滑过渡
3. 默认呼吸周期 4 秒
4. 提交: `feat: implement breathing animation`

---

## Task 4: 实现点击跳动效果

**Goal:** 点击粒子云时跳一下 + 短暂放大

**Files:**
- Modify: `src/components/ParticleCloud.tsx`
- Create: `src/hooks/usePulseEffect.ts`

**Steps:**
1. 实现跳动：粒子散开 SPREAD_RADIUS*1.5，持续 300ms
2. 绑定 onClick 事件，cursor: pointer
3. 提交: `feat: add click pulse effect`

---

## Task 5: 实现拖拽移动

**Goal:** 可拖拽粒子云到任意位置，边缘自动吸附

**Files:**
- Create: `src/hooks/useDraggable.ts`
- Modify: `src/components/ParticleCloud.tsx`

**Steps:**
1. 实现 useDraggable：鼠标按下开始拖拽，限制屏幕范围内
2. 贴边吸附：x < 100 吸附左边缘，y > screenHeight - 100 吸附底部
3. 提交: `feat: add drag-to-reposition interaction`

---

## Task 6: 实现三种状态机

**Goal:** 管理 breathing/pulsing/bubbling 三态切换

**Files:**
- Create: `src/stores/particleState.ts`
- Modify: `src/components/ParticleCloud.tsx`

**Steps:**
1. 定义 ParticleState 类型枚举
2. 实现状态转换逻辑
3. breathing → pulsing: 外部事件
4. pulsing → breathing: 300ms 后自动
5. breathing → bubbling: 事件触发
6. bubbling → breathing: 气泡消散后
7. 提交: `feat: implement particle state machine`

---

## Task 7: 实现气泡浮现效果

**Goal:** 事件触发时粒子膨胀后气泡浮现

**Files:**
- Create: `src/components/Bubble.tsx`
- Modify: `src/components/ParticleCloud.tsx`

**Steps:**
1. Bubble 组件：100x80 白皙半透明，模糊边缘
2. 动画：膨胀 → 气泡出现 → 停留 2 秒 → 消散
3. 提交: `feat: add bubble emergence effect`

---

## Task 8: 实现事件触发机制

**Goal:** 前端事件总线，触发气泡显示

**Files:**
- Create: `src/events/eventBus.ts`
- Modify: `src/components/ParticleCloud.tsx`

**Steps:**
1. 定义 EventType: task_complete, important_found, daily_summarize
2. 实现事件订阅/发布
3. 气泡触发流程：监听 → bubbling 状态 → 显示 → 恢复 breathing
4. 提交: `feat: add event trigger system for bubbles`

---

## Task 9: 心跳调度器连接

**Goal:** Rust 后端心跳定期触发前端 pulsing 状态

**Files:**
- Create: `src-tauri/src/heartbeat.rs`
- Create: `src/hooks/useHeartbeat.ts`

**Steps:**
1. Rust 端：每 60 秒发送心跳事件
2. 前端：监听心跳，触发 pulsing
3. 提交: `feat: connect heartbeat scheduler to particle states`

---

## Task 10: 本地数据存储

**Goal:** 存储活动日志、记忆、设置到本地 JSON

**Files:**
- Create: `src-tauri/src/storage.rs`
- Create: `src/hooks/useStorage.ts`

**Steps:**
1. 定义 Storage 接口：events, memories, settings
2. 使用 tauri::fs 在应用数据目录读写 JSON
3. 提交: `feat: add local storage for events and memories`

---

## Task 11: 用户设置面板

**Goal:** 可配置呼吸速度和昼夜时间节点

**Files:**
- Create: `src/components/Settings.tsx`
- Modify: `src/App.tsx`

**Steps:**
1. 设置项：breathSpeed (slow/normal/fast), timeZones (morning/evening)
2. 持久化到本地存储
3. 提交: `feat: add settings panel`

---

## Task 12: 窗口配置与打包

**Goal:** 配置 Tauri 窗口实现边缘悬浮

**Files:**
- Modify: `src-tauri/tauri.conf.json`

**Steps:**
1. 窗口配置：透明、无装饰、永远置顶、不可改变大小
2. 初始位置：左下角
3. 构建: `npm run tauri build`
4. 提交: `chore: configure window for edge positioning`

---

**Plan extracted from conversation. 12 tasks total.**
