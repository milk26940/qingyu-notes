# 轻羽阶段 3/4 设计规格

日期：2026-08-24

## 阶段 3：Notion 式数据库

数据库页面和数据库记录继续使用统一 Page 模型，新增 `database` 与 `databaseRecord` 类型。数据库定义保存于 `.noteapp/databases/<database-id>.json`，包含字段、表格视图、筛选和排序。记录页面的 `databaseId` 与属性保存到 Markdown 前置元数据，正文仍为 Markdown 权威数据。

字段支持文本、数字、单选、多选、日期、复选框和页面关系。关系值只保存 Page UUID。表格视图支持创建记录、内联修改标题和属性、组合筛选与多字段排序；记录可打开为完整 Markdown 页面。视图模型预留 `table`、`board`、`calendar`，本阶段只允许创建和渲染 `table`。

## 阶段 4：Codex Agent

轻羽继续复用本机 `codex app-server`，初始化时启用 `experimentalApi`。Runtime 保留子进程 stdin/stdout，使用递增 JSON-RPC id 和待处理请求表支持并发请求。支持 `thread/start`、`thread/resume`、`turn/start`、`turn/interrupt`，并在进程异常退出后重启和恢复已登记线程。

每个轻羽对话有独立 ID、Codex `threadId`、标题、上下文范围和 JSONL 投影。上下文范围为全库、当前页面或当前数据库。轻羽只展示自己登记的对话。

`dynamicTools` 注册 `page.search/read/create/update/move/archive`、`database.query/update_schema/create_record/update_record`、`relation.link/unlink/backlinks`。工具请求由 Rust 领域服务执行，写操作先生成 diff；删除、覆盖、批量移动、数据库结构修改和恢复进入审批队列。动态工具不可用时线程降级为只读模式。

前端消费 `item/agentMessage/delta`、`turn/plan/updated`、`turn/diff/updated`、`item/started`、`item/completed` 和 `turn/completed`，分别展示流式文本、计划、diff、工具进度和终态。对话、消息、工具摘要、审批和变更摘要写入 `.noteapp/conversations/<conversation-id>.jsonl`，不保存隐藏推理、凭据或 Codex 内部数据库。

## 验证

Rust 测试覆盖字段验证、记录查询、筛选排序、关系 UUID、对话投影、RPC 路由、工具审批和恢复状态。前端测试覆盖表格属性编辑和 Agent 流式状态。最后用临时笔记库与本机 App Server 完成真实线程创建、回合、事件和恢复验证。
