# 轻羽页面系统第二阶段设计

日期：2026-08-24

## 目标

实现本地优先的 Markdown 页面系统：页面创建、编辑、保存、页面树、父子关系、拖拽移动、UUID 稳定链接、反向链接、搜索索引、回收站和基础历史记录。

## 数据边界

- Markdown 文件是正文和页面元数据的权威来源。
- 默认笔记库为 `%USERPROFILE%/Documents/轻羽笔记库`，Rust 提供选择和初始化其他目录的命令。
- 每个页面使用一个 Markdown 文件，文件名来自标题的安全化版本；UUID 写入 YAML 前置元数据。
- `.noteapp/vault.json` 保存笔记库版本，`.noteapp/history/<page-id>.jsonl` 保存页面版本摘要，`.noteapp/trash/` 保存删除页面的可恢复副本。
- 搜索索引先使用 Rust 内存索引并可从 Markdown 重建；接口不把索引当权威数据。

## 页面模型和命令

页面模型包含 `id`、`title`、`parentId`、`kind`、`content`、`createdAt`、`updatedAt`、`trashedAt` 和 `sortOrder`。Rust 领域服务提供 `vault_init`、`page_list`、`page_get`、`page_create`、`page_update`、`page_move`、`page_trash`、`page_restore`、`page_history`、`page_search` 和 `page_links`。所有写入先写临时文件，再原子替换，并同步历史记录和内存索引。

## 链接和搜索

编辑器支持 `[[页面标题]]` 语法；保存时解析标题并生成稳定的目标 UUID 元数据索引。页面重命名和移动只改变路径与显示标题，不改变 UUID。索引维护正向链接、反向链接和标题/正文分词匹配；搜索结果返回页面 UUID、标题、路径和匹配片段。

## 界面行为

左栏显示可展开页面树，点击节点打开页面；新增页面默认作为当前页面的子页面。拖拽节点调用 `page_move`，成功后刷新树。顶部提供保存状态、新建和回收站入口。删除进入回收站，恢复回原父页面；永久删除不在本阶段提供。页面标题和正文均可编辑，保存通过防抖调用 Tauri 命令。

## 验证

Rust 单元测试覆盖元数据解析、路径安全、创建/更新/移动、链接反链、回收站恢复和历史版本。React 测试覆盖页面树选择、新建、编辑保存状态和搜索结果。最终运行前端测试/构建、Rust 测试、格式检查和 Clippy。
