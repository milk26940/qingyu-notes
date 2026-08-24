# QingYu Notion Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 完成轻羽的 Notion 式 Markdown 编辑、无限子页面、图片附件、页面/区块引用、页面内 AI、Agent 历史切换、Inter 字体与文字字号调节，并交付更新后的 Windows 安装包。

**Architecture:** Markdown 与 JSON 继续作为权威数据。Rust PageRepository 负责安全的页面、附件和链接领域操作；CodeMirror 6 扩展负责编辑装饰、提及、区块定位和行内 AI 候选；Zustand 只保存 UI 状态。所有稳定引用使用 Page/Block UUID，Agent 继续复用本机 Codex App Server。

**Tech Stack:** React 18、TypeScript、CodeMirror 6、Zustand、Radix UI、TailwindCSS、Tauri 2、Rust、serde、Tokio、Vitest、NSIS。

---

### Task 1: 修复 Agent Scope 并建立历史切换界面

**Files:**
- Modify: `src-tauri/src/codex/conversation.rs`
- Modify: `src/components/agent/AgentPanel.tsx`
- Modify: `src/lib/tauri/conversations.ts`
- Modify: `src/stores/agentStore.ts`
- Modify: `src/stores/agentStore.test.ts`
- Create: `src/components/agent/AgentPanel.test.tsx`

- [ ] **Step 1: 写 scope 兼容失败测试**

在 Rust 测试中分别反序列化 `pageId/page_id` 与 `databaseId/database_id`，并断言重新序列化只输出 camelCase：

```rust
let current: ConversationScope = serde_json::from_value(json!({
    "kind": "page", "pageId": "page-1"
})).unwrap();
let legacy: ConversationScope = serde_json::from_value(json!({
    "kind": "page", "page_id": "page-1"
})).unwrap();
assert_eq!(current, legacy);
assert_eq!(serde_json::to_value(current).unwrap()["pageId"], "page-1");
```

- [ ] **Step 2: 运行 Rust 定向测试并确认 RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml conversation_scope_accepts_camel_and_legacy_fields`

Expected: FAIL，因为当前 enum 字段只接受 `page_id/database_id`。

- [ ] **Step 3: 为 enum 字段增加 rename 与 alias**

```rust
pub enum ConversationScope {
    Vault,
    Page {
        #[serde(rename = "pageId", alias = "page_id")]
        page_id: String,
    },
    Database {
        #[serde(rename = "databaseId", alias = "database_id")]
        database_id: String,
    },
}
```

- [ ] **Step 4: 写 Agent 历史列表失败测试**

渲染两个对话，点击“历史对话”，验证标题、范围、时间均可见；点击第二项后断言 `open("conversation-2")`：

```tsx
fireEvent.click(screen.getByRole("button", { name: "历史对话" }));
expect(screen.getByText("项目规划")).toBeVisible();
fireEvent.click(screen.getByRole("button", { name: /打开 项目规划/ }));
expect(openConversation).toHaveBeenCalledWith("conversation-2");
```

- [ ] **Step 5: 实现历史弹出列表并移除横向标签**

使用 Radix DropdownMenu，列表按 `updatedAt` 倒序，显示标题、`scopeLabel()`、格式化时间与当前标记。点击调用 store 的 `open()` 并关闭菜单。

- [ ] **Step 6: 防止无页面时发送非法 scope**

`scope` 计算在 page/database 不可用时返回 `{ kind: "vault" }`；类型继续只暴露 camelCase。

- [ ] **Step 7: 运行相关测试并提交**

Run: `cargo test --manifest-path src-tauri/Cargo.toml conversation_scope`

Run: `npm test -- --run src/stores/agentStore.test.ts src/components/agent/AgentPanel.test.tsx`

Commit: `fix: restore agent scopes and conversation history`

### Task 2: 内置 Inter 与文字字号快捷键

**Files:**
- Modify: `package.json`
- Modify: `package-lock.json`
- Modify: `src/styles/index.css`
- Modify: `src/components/editor/editorExtensions.ts`
- Create: `src/stores/appearanceStore.ts`
- Create: `src/stores/appearanceStore.test.ts`
- Modify: `src/app/App.tsx`

- [ ] **Step 1: 安装字体资源**

Run: `npm install @fontsource-variable/inter`

在 `src/styles/index.css` 首行导入 `@fontsource-variable/inter`，将 UI 与编辑器正文的首选字体设为 `"Inter Variable"`；代码继续使用等宽字体。

- [ ] **Step 2: 写字号 reducer 和持久化失败测试**

```ts
expect(nextFontScale(100, "increase")).toBe(110);
expect(nextFontScale(150, "increase")).toBe(150);
expect(nextFontScale(80, "decrease")).toBe(80);
expect(nextFontScale(130, "reset")).toBe(100);
```

测试初始化读取 `qingyu.fontScale`，变化后写回 localStorage，并同步设置 `--qy-font-scale`。

- [ ] **Step 3: 实现 appearanceStore**

Store 暴露 `fontScale`、`increaseFont`、`decreaseFont`、`resetFont`、`applyFontScale`。CSS 使用：

```css
:root { --qy-font-scale: 1; }
.qy-text-xs { font-size: calc(11px * var(--qy-font-scale)); }
.qy-text-sm { font-size: calc(12px * var(--qy-font-scale)); }
.qy-text-md { font-size: calc(13px * var(--qy-font-scale)); }
.qy-text-editor { font-size: calc(15px * var(--qy-font-scale)); }
```

将现有用户可见固定字号替换为这组语义类；图标、面板宽度和间距保持不变。

- [ ] **Step 4: 注册全局快捷键**

在 App effect 中监听 `keydown`，仅处理 `ctrlKey && !altKey && !isComposing`：`+`/`=` 增大、`-` 减小、`0` 重置，并 `preventDefault()`。

- [ ] **Step 5: 让 CodeMirror 读取同一 CSS 变量**

```ts
"&": {
  fontSize: "calc(15px * var(--qy-font-scale))",
  fontFamily: '"Inter Variable", Inter, sans-serif',
}
```

- [ ] **Step 6: 运行测试、构建并提交**

Run: `npm test -- --run src/stores/appearanceStore.test.ts`

Run: `npm run build`

Commit: `feat: add Inter typography and font scaling`

### Task 3: 完善无限子页面与数据库条目正文

**Files:**
- Modify: `src-tauri/src/pages/repository.rs`
- Modify: `src/stores/pagesStore.ts`
- Modify: `src/components/pages/PageTree.tsx`
- Modify: `src/components/pages/PageToolbar.tsx`
- Modify: `src/app/App.tsx`
- Create: `src/components/pages/PageTree.test.tsx`

- [ ] **Step 1: 保留并扩展循环校验测试**

建立三层页面 A/B/C，断言 `move_page(A, C)` 返回 `page_invalid_parent`，并验证第四层可创建和读取。

- [ ] **Step 2: 在页面树节点增加子页面按钮**

每个节点 hover 时显示 `FilePlus2` 图标，调用 `create("未命名页面", page.id)`；工具栏的“新建页面”在有 activePage 时同样创建子页。

- [ ] **Step 3: 在正文顶部显示子页面入口与子页面列表**

创建 `PageChildren` 小组件，从 `pages.filter(parentId === activePage.id)` 得到子页面，提供创建和打开操作，不使用嵌套卡片。

- [ ] **Step 4: 数据库条目页共用正文编辑器**

App 路由仅对 `kind === "database"` 渲染表格；`databaseRecord` 进入包含属性摘要、标题、子页面与 MarkdownEditor 的文档布局。

- [ ] **Step 5: 运行测试并提交**

Run: `cargo test --manifest-path src-tauri/Cargo.toml page_lifecycle_preserves_uuid_and_supports_trash_restore`

Run: `npm test -- --run src/components/pages/PageTree.test.tsx src/app/App.test.tsx`

Commit: `feat: complete nested pages and record documents`

### Task 4: 图片附件领域服务与编辑器插入

**Files:**
- Modify: `src-tauri/src/pages/repository.rs`
- Modify: `src-tauri/src/commands/pages.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/backup/repository.rs`
- Modify: `src/lib/tauri/pages.ts`
- Create: `src/components/editor/imageAttachments.ts`
- Modify: `src/components/editor/MarkdownEditor.tsx`
- Modify: `src/components/editor/MarkdownEditor.test.tsx`

- [ ] **Step 1: 写附件安全失败测试**

测试 PNG/JPEG/GIF/WebP 魔数、20 MiB 上限、危险文件名和页面不存在。成功时文件只能写到 `.noteapp/assets/<page-id>/<uuid>.<ext>`，返回 `/.noteapp/assets/...`。

- [ ] **Step 2: 实现 `store_asset` 与 Tauri command**

```rust
pub fn store_asset(
    &self,
    page_id: &str,
    original_name: &str,
    bytes: &[u8],
) -> Result<String, AppError>
```

Command `page_store_asset(page_id, original_name, bytes: Vec<u8>)` 返回 Markdown 路径；验证页面存在后才写入，并使用临时文件原子重命名。

- [ ] **Step 3: 实现粘贴、拖放和文件选择**

`imageAttachments.ts` 负责筛选 `image/*`、读取 ArrayBuffer、调用 IPC、生成 `![alt](path)`。文件选择按钮使用隐藏 `<input type="file" accept="image/png,image/jpeg,image/gif,image/webp">`。

- [ ] **Step 4: 在 CodeMirror 插入图片 Markdown**

按当前 selection 替换或插入，失败时保持文档不变并把错误交给页面 Store 显示。

- [ ] **Step 5: 运行测试并提交**

- 在备份测试中创建 `.noteapp/assets/<page-id>/image.png`，断言整库快照和 ZIP 导出都包含该相对路径，恢复后字节一致。

Run: `cargo test --manifest-path src-tauri/Cargo.toml asset`

Run: `cargo test --manifest-path src-tauri/Cargo.toml backup`

Run: `npm test -- --run src/components/editor/MarkdownEditor.test.tsx`

Commit: `feat: add page image attachments`

### Task 5: 页面提及与稳定页面链接

**Files:**
- Modify: `src-tauri/src/pages/repository.rs`
- Create: `src/components/editor/pageMentions.ts`
- Modify: `src/components/editor/editorExtensions.ts`
- Modify: `src/components/editor/MarkdownEditor.tsx`
- Modify: `src/components/editor/MarkdownEditor.test.tsx`

- [ ] **Step 1: 扩展链接解析测试**

输入 `[新标题](qingyu://page/page-uuid)`，断言 `links()` 和 `backlinks()` 都按 UUID 识别，页面移动与重命名后仍命中。

- [ ] **Step 2: 实现 qingyu 页面链接解析**

`extract_link_tokens` 同时支持现有 `[[Title|uuid]]` 与 `qingyu://page/<uuid>`；旧格式继续可读，新插入统一使用 Markdown 内部链接。

- [ ] **Step 3: 实现 `@` 补全**

CodeMirror autocomplete source 仅在 `@query` 上激活，选项显示页面标题与层级路径，apply 插入：

```markdown
[最新页面标题](qingyu://page/<uuid>)
```

- [ ] **Step 4: 实现点击跳转与失效样式**

ViewPlugin 把内部链接显示为页面提及；点击调用 `pagesStore.open(uuid)`。UUID 不存在时显示“页面不存在”且不抛异常。

- [ ] **Step 5: 运行测试并提交**

Run: `cargo test --manifest-path src-tauri/Cargo.toml links`

Run: `npm test -- --run src/components/editor/MarkdownEditor.test.tsx`

Commit: `feat: add stable page mentions`

### Task 6: 区块身份、复制链接与精准定位

**Files:**
- Create: `src/components/editor/blockIdentity.ts`
- Modify: `src/components/editor/editorExtensions.ts`
- Modify: `src/components/editor/MarkdownEditor.tsx`
- Modify: `src/components/pages/PageToolbar.tsx`
- Modify: `src/stores/pagesStore.ts`
- Create: `src/components/editor/blockIdentity.test.ts`

- [ ] **Step 1: 写块标记纯函数测试**

`ensureBlockIds(markdown)` 为无标记的非空块添加 UUID，保留已有 UUID，并保证第二次调用结果不变。`findBlock(markdown, id)` 返回目标块范围。

- [ ] **Step 2: 实现块标记与隐藏装饰**

块标记格式固定为 `<!-- qy:block:<uuid> -->`。CodeMirror decoration 隐藏标记行，但在光标进入该行时显示源码，避免内容不可编辑。

- [ ] **Step 3: 实现复制链接**

页面工具栏“复制页面链接”写入 `qingyu://page/<page-id>`；编辑器当前块菜单写入 `qingyu://page/<page-id>#<block-id>`。

- [ ] **Step 4: 实现启动链接解析与定位**

App 解析待打开内部链接，调用 `open(pageId)`；MarkdownEditor 收到 `targetBlockId` 后滚动到对应 DOM 块并应用短暂高亮类。找不到块时页面 Store 显示退化提示。

- [ ] **Step 5: 运行测试并提交**

Run: `npm test -- --run src/components/editor/blockIdentity.test.ts src/components/editor/MarkdownEditor.test.tsx`

Commit: `feat: add block links and precise navigation`

### Task 7: Notion 式 Markdown 装饰

**Files:**
- Create: `src/components/editor/notionDecorations.ts`
- Modify: `src/components/editor/editorExtensions.ts`
- Modify: `src/styles/index.css`
- Modify: `src/components/editor/MarkdownEditor.test.tsx`

- [ ] **Step 1: 写可见装饰测试**

渲染包含标题、加粗、列表、引用、代码和图片的 Markdown，断言编辑器存在对应语义 class；聚焦语法附近时源码仍可编辑。

- [ ] **Step 2: 实现语法树驱动 Decorations**

使用 `syntaxTree(view.state)` 生成标题、强调、链接、图片和块标记 decoration；只隐藏安全的标记字符，不改变文档内容。

- [ ] **Step 3: 实现图片 Widget 与缺图占位**

图片通过受控 Tauri 资源 URL 或读取 command 显示，稳定尺寸并限制最大宽度。加载失败显示替代文本和“图片不可用”。

- [ ] **Step 4: 运行测试、构建并提交**

Run: `npm test -- --run src/components/editor/MarkdownEditor.test.tsx`

Run: `npm run build`

Commit: `feat: add Notion-style markdown presentation`

### Task 8: 页面内 AI 生成、总结与改写

**Files:**
- Modify: `src/components/editor/MarkdownEditor.tsx`
- Create: `src/components/editor/InlineAgentMenu.tsx`
- Create: `src/stores/inlineAgentStore.ts`
- Create: `src/stores/inlineAgentStore.test.ts`
- Modify: `src/stores/agentStore.ts`
- Modify: `src/components/agent/AgentPanel.tsx`

- [ ] **Step 1: 写候选变更状态测试**

测试生成在 selection 后插入、总结/改写替换 selection；请求失败和拒绝保持原文；接受只应用一次并清除候选。

- [ ] **Step 2: 实现选区菜单与请求构造**

菜单仅在非空选区显示“总结/改写”，始终提供“生成”。请求包含 page UUID、选区文本、前后有限上下文和明确的“只返回候选正文”约束，不包含库外文件。

- [ ] **Step 3: 复用当前页面 Agent 对话**

查找 scope 为当前 pageId 的最近对话；没有则 `create({ kind: "page", pageId }, "改写 · 页面标题")`。通过现有 App Server 流式事件收集候选，不直接调用 page.update 工具。

- [ ] **Step 4: 展示 diff 与接受/拒绝**

InlineAgent Store 保存 `{ from, to, original, proposed, status }`。编辑器下方显示紧凑 diff；接受 dispatch CodeMirror transaction，拒绝清除候选。接受后的普通 `onChange/save` 写入页面历史。

- [ ] **Step 5: 运行测试并提交**

Run: `npm test -- --run src/stores/inlineAgentStore.test.ts src/components/editor/MarkdownEditor.test.tsx src/stores/agentStore.test.ts`

Commit: `feat: add inline Codex writing actions`

### Task 9: 完整验收与 Windows 发布

**Files:**
- Modify only files required by discovered verification failures.
- Output: `src-tauri/target/release/bundle/nsis/轻羽_0.1.0_x64-setup.exe`

- [ ] **Step 1: 运行全部静态与自动化验证**

Run:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
npm run build
npm test
git diff --check
```

Expected: 全部退出码 0。

- [ ] **Step 2: 使用临时笔记库做集成验收**

验证四层子页面、数据库条目正文、粘贴图片、`@` 引用、页面重命名/移动后的跳转、区块复制与定位、三种行内 AI 操作、Agent 历史切换、旧 scope 读取和字号重启恢复。

- [ ] **Step 3: 构建安装包**

Run: `npm run tauri build`

Expected: 生成 `轻羽_0.1.0_x64-setup.exe`。

- [ ] **Step 4: 更新 `D:\轻羽` 并验证真实进程**

只关闭轻羽自己的进程树，静默安装新 NSIS，重新启动。确认轻羽窗口可见、Codex 后台连接、无可见命令行窗口，并验证用户笔记库不被测试数据修改。

- [ ] **Step 5: 提交最终修复并检查干净工作树**

Commit: `fix: complete Notion editor release verification`
