# 轻羽 Qingyu

本地优先的桌面笔记软件，将区块编辑、页面数据库和 AI 对话放在同一个工作空间。

## 主要功能

- **区块编辑**：正文、标题、列表、待办、引用、表格、代码块、Mermaid 和图片。
- **页面管理**：页面树、页面链接、反向链接、目录、搜索及导入导出。
- **页面数据库**：字段、记录、筛选、排序和页面关联。
- **AI 助手**：结合输入、选中文本和附件按需读取笔记，通过页面工具执行修改。
- **对话时间线**：按轮次展示进度、工具执行、最终答复及修改前后对照；支持待发送消息排队、编辑和移除。
- **本地存储**：SQLite 保存页面、区块、数据库和对话，图片和文件独立保存；支持备份与恢复。

当前主要在 Windows 上开发和验证。其他系统的构建和运行尚未完成验证。

## 本地运行

需要 Node.js 22 或更新的兼容版本、Rust 稳定工具链，以及 Tauri 2 对应平台的构建依赖。Windows 还需要 Microsoft C++ Build Tools 和 WebView2 Runtime。

~~~powershell
git clone https://github.com/milk26940/qingyu-notes.git
cd qingyu-notes
npm ci
npm run tauri dev
~~~

AI 对话依赖本机可用的 Codex 命令及登录配置。仓库不提供 API 密钥或用户登录凭据。

## 构建与检查

~~~powershell
# 构建前端
npm run build

# 运行前端检查
npm test

# 运行 Rust 测试
cargo test --manifest-path src-tauri/Cargo.toml

# 构建桌面程序与安装包
npm run tauri build
~~~

日常修改优先使用开发模式。独立开发程序需要嵌入前端资源时，可在前端构建后执行：

~~~powershell
cargo build --manifest-path src-tauri/Cargo.toml --bin qingyu-notes --features tauri/custom-protocol --locked
~~~

## 数据存储

- 程序目录下的 `data/qingyu.sqlite3` 是页面、区块、数据库和对话的权威数据源。
- 图片和文件位于 `data/attachments`；Markdown 用于导入导出。
- Windows 默认备份目录为 `%LOCALAPPDATA%\QingYu\Backups`。
- 开发目录和安装目录各自使用自己的本地数据；切换程序位置前请确认数据目录并做好备份。
- AI 修改使用轻羽提供的业务工具，并经过版本检查、变更记录和相应备份检查点。
- RAG 和知识 Wiki 功能已停用；历史设计文件可能包含旧方案，以当前实现为准。

个人笔记、附件、备份、凭据、依赖目录和编译产物不随源码上传。

## 项目结构

| 路径 | 用途 |
| --- | --- |
| `src/components/` | 编辑器、页面、数据库与 Agent 界面 |
| `src/stores/` | 前端状态管理 |
| `src/lib/tauri/` | 前后端调用接口 |
| `src-tauri/src/` | Rust 后端、SQLite、备份及 Agent 工具 |
| `public/licenses/` | 随附资源的许可证 |
| `src/assets/` | 字体和静态资源 |
| `scripts/` | 导入辅助脚本 |
| `docs/` | 设计说明和历史实施计划 |

技术栈：React、TypeScript、Tiptap / ProseMirror、CodeMirror 6、Zustand、Tauri 2、Rust 和 SQLite。

## 源码与许可证

本仓库公开源码，项目本身暂未指定开源许可证。随附字体及第三方依赖仍遵循各自许可证，思源黑体许可证见 `public/licenses/LICENSE-SourceHanSansCN.txt`。
