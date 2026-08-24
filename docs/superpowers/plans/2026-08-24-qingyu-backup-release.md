# QingYu Backup And Release Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为轻羽加入可去重的本地快照、单页/整库恢复、ZIP 导入导出、危险 Agent 写入前检查点和 Windows 安装包。

**Architecture:** Rust `backup` 模块以笔记库文件为权威数据，按 SHA-256 将内容写入 `%LOCALAPPDATA%\QingYu\backups\<vault-id>\objects`，快照清单写入 `snapshots`。恢复先在备份目录 staging 中重建并校验，再原子覆盖目标文件；Tauri commands 暴露列表、创建、恢复和 ZIP 操作，React 仅保存显示状态。

**Tech Stack:** Rust、serde、sha2、zip、Tauri commands、React 18、Zustand、Vitest、NSIS

---

### Task 1: 内容寻址快照仓库

**Files:**
- Create: `src-tauri/src/backup/model.rs`
- Create: `src-tauri/src/backup/repository.rs`
- Create: `src-tauri/src/backup/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`

- [ ] 定义 `BackupScope`、`BackupManifest`、`BackupEntry` 和 `BackupRepository`，清单只保存相对路径、SHA-256 和字节数。
- [ ] 写失败测试：相同内容在两个快照中只生成一个对象；篡改对象后恢复必须失败。
- [ ] 实现递归扫描、路径归一化、对象去重、临时清单原子替换和对象哈希校验。
- [ ] 运行 `cargo test backup`，期望全部通过。

### Task 2: 单页与整库恢复

**Files:**
- Modify: `src-tauri/src/backup/repository.rs`
- Test: `src-tauri/src/backup/repository.rs`

- [ ] 写失败测试：整库恢复回到快照内容，单页恢复不修改其他页面。
- [ ] 实现 `restore(snapshot_id)`：先在 `staging/<uuid>` 重建并验证，再逐文件原子替换；整库恢复删除快照后新增的普通库文件，但保留恢复安全检查点。
- [ ] 将恢复前自动检查点标记为 `beforeRestore`。
- [ ] 运行 `cargo test backup`，期望全部通过。

### Task 3: ZIP 导入导出与 IPC

**Files:**
- Create: `src-tauri/src/commands/backup.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `src/lib/tauri/backups.ts`
- Create: `src/stores/backupStore.ts`

- [ ] 添加 `backup_checkpoint`、`backup_list`、`backup_restore`、`backup_export_zip` 和 `backup_import_zip` commands。
- [ ] ZIP 导出包含 Markdown、附件、数据库定义和轻羽对话 JSONL；明确排除临时文件、备份对象、Codex/CCSwitch 配置和数据库。
- [ ] ZIP 导入拒绝绝对路径和 `..`，先解压到 staging 并校验，再建立检查点和应用文件。
- [ ] 增加 Rust ZIP 路径穿越测试并运行 `cargo test backup`。

### Task 4: 危险 Agent 写入检查点

**Files:**
- Modify: `src-tauri/src/codex/runtime.rs`

- [ ] 在批准 `page.update`、`page.archive`、`database.update_schema`、`relation.unlink` 后、执行工具前创建 `agentDangerousWrite` 快照。
- [ ] 备份失败时返回结构化失败，不执行业务写入。
- [ ] 增加危险工具分类和备份失败不写入测试。

### Task 5: 备份界面与发布

**Files:**
- Modify: `src/components/layout/Topbar.tsx`
- Create: `src/components/backup/BackupDialog.tsx`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/icons/*`

- [ ] 增加备份按钮、快照列表、立即备份、恢复和 ZIP 导入导出入口；恢复在操作时二次确认。
- [ ] 用用户提供的羽毛图片生成 Tauri 全套图标，保持产品名“轻羽”和 NSIS target。
- [ ] 运行 `npm run build`、`npm test`、`cargo fmt -- --check`、`cargo test`、`cargo clippy -- -D warnings`。
- [ ] 运行 `npm run tauri build`，确认生成可安装的 NSIS 包。

### Self-Review

- [ ] 备份不触碰 `auth.json`、Codex SQLite、CCSwitch SQLite。
- [ ] 所有归档路径均为笔记库相对路径，恢复和 ZIP 导入拒绝路径穿越。
- [ ] 危险 Agent 写入在备份失败时被阻止。
- [ ] 单页和整库恢复均由测试覆盖。
