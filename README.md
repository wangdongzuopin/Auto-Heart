# Auto-Heart

一个常驻桌面的 AI 编程陪伴助手。
![alt text](image.png)

Auto-Heart 试图把“会话式 AI”变成“持续理解你当前工作状态的桌面协作者”：它以一个悬浮在桌面边缘的 Orb 作为入口，持续感知你的文件变更、前台工作上下文、今日意图和最近对话，在合适的时机给出提醒、整理任务、生成日报，尽量做到低打扰但有存在感。

> 当前项目仍在快速迭代中，README 以仓库内已实现能力为准，部分设计蓝图和长期规划仍在持续落地。

## 项目定位

这个项目并不是一个“再包装一次聊天框”的 AI 工具，而是一个面向开发者日常工作流的本地桌面助手：

- 它关注“你今天在做什么”，而不是只回答单轮提问
- 它能结合文件变化、工作片段和会话内容，形成更连续的上下文
- 它强调本地感知、本地存储和可配置模型路由，适合个人工作台场景
- 它尝试在“陪伴感”和“打扰感”之间找到平衡

## 当前能力

### 已实现

- `桌面 Orb`
  常驻桌面右下角，作为主入口与状态载体
- `主窗口工作台`
  提供 Today、Conversation、Settings 三个核心页面
- `文件感知`
  监听指定目录内的文件变更，写入本地数据库并在界面中展示
- `前台活动感知`
  记录当前活跃应用、窗口标题、活动分类和工作片段
- `今日任务抽取`
  从意图文档或聊天内容中解析出结构化任务列表
- `多模型路由`
  支持 Kimi、Qwen、MiniMax、OpenAI、Claude、DeepSeek、OpenRouter、Ollama
- `本地会话记忆`
  会话可持久化保存，并结合项目记忆、工作片段和文件上下文参与回复
- `日报生成与发送`
  支持生成当日日报，并发送到钉钉或飞书 Webhook
- `开机自启与运行配置`
  支持监听目录、数据目录、静默模式、下班时间等配置

### 规划中 / 持续增强

- 更完整的语义地图视图
- 更稳定的主动提醒策略
- 更精细的项目级上下文建模
- 更成熟的跨平台体验
- 更完善的隐私控制、导出与诊断能力

## 界面与交互

Auto-Heart 当前主要包含两个界面层：

### 1. Orb 悬浮球

- 默认常驻桌面
- 透明、无边框、始终置顶
- 更像一个“状态入口”而不是完整工作区

### 2. 主窗口

- `Today`
  查看当日文件变更、活动快照、工作统计和日报
- `Conversation`
  与 Auto-Heart 持续对话，复用历史上下文和本地工作记忆
- `Settings`
  配置模型、API Key、监听目录、意图文档、Webhook、数据目录等

## 核心设计思路

### 三层 heartbeat 机制

项目围绕多层节奏任务组织能力：

- `浅层 heartbeat`
  高频本地感知，负责前台窗口、活动快照、自然节点检查等
- `中层 heartbeat`
  负责意图解析、文件变化语义分析、任务更新等轻量 AI 工作
- `深层 heartbeat`
  负责日报整理、离岗判断和更重的总结型任务

这种分层方式的目标是：

- 高频感知尽量本地完成，降低成本和延迟
- AI 调用只在必要时发生
- 将“实时反馈”和“深度总结”拆开处理

### 本地优先的数据组织

项目当前使用 SQLite 持久化核心上下文，包括但不限于：

- `intent_history`
  今日意图与解析后的任务
- `file_changes`
  文件变更记录
- `activity_snapshots`
  前台应用与工作快照
- `work_sessions`
  聚合后的工作片段
- `operation_log`
  文件变化的语义意图分析结果
- `conversation_memory`
  单会话记忆
- `project_memory`
  项目级长期记忆
- `daily_reports`
  日报草稿、确认态与发送态

## 技术栈

| 层 | 技术 |
| --- | --- |
| 桌面框架 | Tauri 2 |
| 前端 | React 18 + TypeScript + Vite |
| 后端 | Rust |
| 本地存储 | SQLite + rusqlite |
| 文件监听 | notify |
| 网络请求 | reqwest |
| 模型接入 | OpenAI-compatible API + Ollama |

## 项目结构

```text
.
├─ auto-heart/                # 主应用目录
│  ├─ src/                    # React 前端
│  │  ├─ components/          # Orb、气泡等 UI 组件
│  │  ├─ hooks/               # 设置、主题、消息队列等 hooks
│  │  └─ pages/               # Today / Conversation / Settings
│  ├─ src-tauri/              # Rust + Tauri 后端
│  │  ├─ src/
│  │  │  ├─ lib.rs            # 应用入口、托盘、窗口、heartbeat 启动
│  │  │  ├─ heartbeat.rs      # 感知与调度核心
│  │  │  ├─ commands.rs       # Tauri IPC 命令
│  │  │  ├─ database.rs       # SQLite schema
│  │  │  ├─ model_router.rs   # 多模型路由
│  │  │  ├─ settings.rs       # 配置加载与持久化
│  │  │  ├─ notifier.rs       # Webhook 发送
│  │  │  └─ conversation.rs   # 会话存储
│  │  └─ tauri.conf.json      # Tauri 配置
├─ docs/                      # 设计稿、实现计划等文档
├─ files/                     # 产品/技术方案材料
├─ PROJECT.md                 # 项目理解与设计说明
└─ README.md
```

## 快速开始

### 运行环境

建议准备以下环境：

- Node.js 18+
- npm 9+
- Rust stable
- Tauri 2 开发依赖
- Windows 10/11 优先

> 当前仓库配置和部分能力明显以 Windows 桌面场景为主，虽然部分实现具备跨平台分支，但实际体验仍建议优先在 Windows 上验证。

### 安装依赖

```bash
cd auto-heart
npm install
```

### 开发模式

```bash
npm run start
```

或：

```bash
npm run tauri dev
```

### 构建发行版

```bash
npm run release
```

或：

```bash
npm run tauri build
```

## 配置说明

第一次运行后，可以在应用 `Settings` 页中完成配置。

### 关键配置项

- `模型提供商与模型名`
  用于中层、深层 heartbeat 以及对话能力
- `API Key`
  按所选模型提供商填写
- `Ollama Base URL`
  使用本地模型时填写，例如 `http://localhost:11434`
- `Intent Doc Path`
  意图文档路径，作为每日计划/任务解析输入
- `Watch Paths`
  文件监听目录，建议加入常用项目根目录
- `Data Dir`
  本地数据目录；每日数据库会写入 `YYYY-MM-DD` 子目录
- `Silence Mode`
  控制提醒强度：`focus` / `normal` / `open`
- `Offwork Time`
  用于日报等下班场景判断
- `DingTalk / Feishu Webhook`
  用于发送日报

### 配置文件优先级

项目当前支持从以下位置读取设置：

1. 用户主目录下的 `~/.autoheart`
2. 应用默认数据目录下的 `settings.json`

## 使用建议

为了让 Auto-Heart 的效果更稳定，建议这样使用：

- 为它配置 1 到 3 个你最常工作的项目目录，而不是一次性监听整块磁盘
- 给它一个相对稳定的“意图文档”，例如每日计划 Markdown
- 先把模型配置跑通，再启用日报发送和更主动的提醒
- 将它当作“工作上下文整理器”，而不只是聊天机器人

## 适合的使用场景

- 个人开发者的日常编码记录与复盘
- 独立项目的持续上下文维护
- 需要自动整理日报/工作摘要的研发场景
- 想让 AI 更贴近本地工作流，而不是停留在浏览器聊天窗口

## 当前限制

在公开使用前，建议先了解这些现状：

- 项目还处在快速迭代期，功能边界仍在调整
- 当前实现对 Windows 场景更友好
- 一些设计文档中的能力尚未完全产品化
- 活动感知与文件监听依赖本地环境，效果会随目录配置和系统权限变化
- 使用第三方模型服务时，请自行评估数据发送范围与合规要求

## Roadmap

- 完善语义地图与项目记忆的可视化
- 强化对代码仓库、分支、提交和工作片段的联动理解
- 优化主动提醒的阈值和准确率
- 增加更多本地优先、可解释的上下文策略
- 补齐测试、打包、诊断与发布流程

## 文档

- 项目理解与设计说明：[`PROJECT.md`](./PROJECT.md)
- 设计与实现计划：[`docs/plans`](./docs/plans)

## 开源协作建议

欢迎围绕以下方向参与贡献：

- Bug 修复与稳定性优化
- Windows 之外的平台适配
- UI/UX 改进
- 提醒策略与记忆策略优化
- 文档补充与示例完善

如果你准备提交 PR，建议优先说明：

- 你修改的场景或问题是什么
- 改动会影响哪些模块
- 是否涉及模型调用、数据结构或本地存储行为变化

## License

当前仓库尚未看到明确的开源许可证文件。

如果你准备正式对外开源，建议尽快补充 `LICENSE` 文件；在许可证明确之前，README 中不应默认宣称可自由商用或二次分发。
