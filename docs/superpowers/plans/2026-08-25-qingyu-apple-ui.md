# 轻羽 Apple 风格界面 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将现有轻羽三栏界面改为高对比度的 Apple 极简风格，并加入可持久化的分栏拖拽调宽。

**Architecture:** 使用现有 Zustand 工作区状态作为宽度单一来源，新增独立拖拽手柄组件，通过 CSS 变量统一视觉。业务页面、数据库和 Agent 协议保持不变。

**Tech Stack:** React 18、TypeScript、Zustand、TailwindCSS、Lucide React、Tauri 2

---

### Task 1: 工作区宽度状态

**Files:**
- Modify: `src/stores/workspaceStore.ts`
- Create: `src/stores/workspaceStore.test.ts`

- [ ] 为左右栏增加带边界约束的 `setSidebarWidth` 和 `setAgentPanelWidth`。
- [ ] 从 localStorage 恢复宽度，并在调整时持久化。
- [ ] 验证非法值会回落到允许范围。

### Task 2: 可拖拽分隔手柄

**Files:**
- Create: `src/components/layout/PanelResizeHandle.tsx`
- Create: `src/components/layout/PanelResizeHandle.test.tsx`
- Modify: `src/components/layout/WorkspaceShell.tsx`

- [ ] 实现左右方向的 Pointer Events 拖拽。
- [ ] 提供 `role="separator"`、键盘方向键调节和拖拽高亮。
- [ ] 将手柄作为独立网格列插入三栏布局，折叠时隐藏。

### Task 3: Apple 视觉主题

**Files:**
- Modify: `src/styles/index.css`
- Modify: `src/components/layout/Topbar.tsx`
- Modify: `src/components/layout/Sidebar.tsx`
- Modify: `src/components/pages/DocumentOutline.tsx`
- Modify: `src/components/agent/AgentPanel.tsx`
- Modify: `src/app/App.tsx`

- [ ] 更新颜色、圆角、阴影、间距和 220ms 点击动效。
- [ ] 顶栏加入灰色 Feather 图标和轻羽品牌。
- [ ] 为中间空白页加入羽毛 Logo 卡片。
- [ ] 加深右侧 Agent 全部文字并增强输入框悬浮阴影。

### Task 4: 验证与开发运行

**Files:**
- Modify: `src/components/layout/Topbar.test.tsx`
- Modify: `src/app/App.test.tsx`

- [ ] 运行 `npm test -- --run`，要求全部通过。
- [ ] 运行 `npm run build`，要求 TypeScript 与 Vite 构建成功。
- [ ] 运行 `npm run tauri dev` 启动开发版供用户查看，不运行 release 打包。
