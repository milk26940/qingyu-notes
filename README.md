# 轻羽

轻羽是本地优先的 Notion 式笔记与知识库应用，使用 React、TypeScript、Tiptap/ProseMirror、CodeMirror 6、Zustand、Tauri 2 和 Rust 构建。

## 数据边界

- 安装目录下的 `data/qingyu.sqlite3` 是页面、区块、数据库和对话的权威数据源。
- 图片和文件附件保存在 `data/attachments`；Markdown 用于导入导出。
- 备份默认位于 `%LOCALAPPDATA%\QingYu\Backups`。
- Agent 复用本机 `codex app-server`、现有登录状态和 `config.toml`，不读取、复制或备份 Codex/CCSwitch 凭据与数据库。
- Agent 写入通过轻羽语义工具、审批、备份检查点和笔记库路径约束完成。

## 开发

开发源码位于 `D:\笔记`，正式程序位于 `D:\轻羽\qingyu-notes.exe`。日常调试使用开发模式；修改完成并验证后同步正式程序。

### 目录说明

| 目录 | 用途 |
| --- | --- |
| `src/` | 前端界面、编辑器和状态管理 |
| `src-tauri/src/` | Rust 后端、SQLite 存储和 Agent 工具 |
| `public/`、`src/assets/` | 静态资源、字体和许可文件 |
| `docs/` | 设计说明及实施计划 |
| `scripts/` | 导入等开发辅助脚本 |
| `data/` | 本地开发数据，不提交 Git |
| `node_modules/`、`dist/`、`src-tauri/target/` | 依赖及构建产物，不提交 Git |
| `.local-history/` | 旧源码备份、迁移演练和调试材料，不参与构建、不提交 Git |

2026-09-09 整理前的历史材料位于 `.local-history/cleanup-20260909/`，按原目录名保留，可移回原路径恢复。Git 当前仅登记主工作树；清理目录不代表丢弃或提交现有源码修改。

```powershell
npm install
npm run tauri dev
```

运行测试与发布构建：

```powershell
npm test -- --run
cargo test --manifest-path src-tauri/Cargo.toml
npm run tauri build
```
