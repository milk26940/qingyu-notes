# 轻羽多模态语义检索 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为轻羽增加可配置的 qwen3-vl-embedding、多模态派生索引和混合搜索。

**Architecture:** Rust 后端独立管理配置、密钥、远程调用和 SQLite 向量索引；React 只负责设置与搜索交互。现有 FTS5 保持不变并作为离线降级。

**Tech Stack:** Tauri 2、Rust、rusqlite、reqwest、Windows DPAPI、React、Zustand。

**Spec:** `docs/superpowers/specs/2026-09-08-qingyu-multimodal-semantic-search-design.md`

## Global Constraints

- 不把 API Key 写入源码、SQLite、日志、备份或导出包。
- 不阻塞页面打开、编辑和保存。
- 保留现有 FTS5 精确搜索。
- 只增加最小必要测试。

---

### Task 1: 配置、密钥和远程客户端

**Files:**
- Create: `src-tauri/src/semantic/config.rs`
- Create: `src-tauri/src/semantic/client.rs`
- Create: `src-tauri/src/semantic/mod.rs`
- Modify: `src-tauri/Cargo.toml`

- [ ] 定义可序列化的公开配置和不含密钥的状态。
- [ ] 使用 DPAPI 加密保存 Key，并实现读取和清除。
- [ ] 实现 DashScope 多模态 Embedding 请求与响应解析。
- [ ] 实现脱敏错误和连接测试。

### Task 2: SQLite 派生索引

**Files:**
- Create: `src-tauri/src/semantic/repository.rs`
- Modify: `src-tauri/src/pages/repository.rs`

- [ ] 创建配置、向量和任务表。
- [ ] 使用 SHA-256 内容哈希判断变化区块。
- [ ] 使用 Float32 BLOB 保存向量。
- [ ] 实现重建、状态统计和余弦相似度查询。

### Task 3: Tauri 命令与后台更新

**Files:**
- Create: `src-tauri/src/commands/semantic.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/commands/pages.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] 注册配置、测试、重建、状态和搜索命令。
- [ ] 页面保存后异步更新变化区块。
- [ ] 后台任务失败时不影响页面保存结果。

### Task 4: 设置界面

**Files:**
- Create: `src/lib/tauri/semantic.ts`
- Create: `src/components/settings/SettingsDialog.tsx`
- Modify: `src/components/layout/Topbar.tsx`

- [ ] 增加设置入口和 Apple 风格对话框。
- [ ] 提供 URL、Key、模型、维度、启用、测试连接和重建按钮。
- [ ] Key 默认掩码显示且不会从后端返回明文。

### Task 5: 混合搜索

**Files:**
- Modify: `src/components/pages/PageSearch.tsx`
- Modify: `src/lib/tauri/pages.ts`
- Modify: `src/stores/pagesStore.ts`

- [ ] 增加精确、混合和语义模式。
- [ ] 混合模式在语义失败时保留 FTS5 结果。
- [ ] 搜索结果继续使用页面 ID 跳转。

### Task 6: 最小验证与发布

- [ ] 运行 Rust 语义模块定向测试。
- [ ] 运行前端构建。
- [ ] 构建 Tauri Release。
- [ ] 同步开发版和安装版，并核对 SHA-256。
