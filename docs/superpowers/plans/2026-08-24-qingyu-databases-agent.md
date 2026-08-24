# QingYu Databases And Agent Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add Notion-style table databases and a persistent Codex Agent client with semantic note tools.

**Architecture:** Database definitions are authoritative JSON beside Markdown record pages. A bidirectional Rust App Server client owns threads, turns, events, dynamic tools, approvals, and portable conversation projections; React consumes typed commands and events.

**Tech Stack:** Rust, Tokio, serde, Tauri 2; React 18, Zustand, CodeMirror 6; Codex App Server JSON-RPC.

---

### Task 1: Extend the Page model and add database repository

- [ ] Add database and record page kinds, properties, and database IDs.
- [ ] Add field/option/view/filter/sort definitions and JSON repository.
- [ ] Add create/update schema, create/update record, and query commands.
- [ ] Test every field type, relation UUIDs, filtering, sorting, and persistence.

### Task 2: Build the table database UI

- [ ] Add typed Tauri database adapter and Zustand store.
- [ ] Add database creation in the page tree.
- [ ] Render a dense table with field creation, record creation, inline property editing, filtering, and sorting.
- [ ] Open database record pages in the Markdown editor.
- [ ] Add focused frontend tests and production build verification.

### Task 3: Upgrade Codex Runtime to bidirectional JSON-RPC

- [ ] Retain stdin and route response IDs through pending request channels.
- [ ] Initialize with `experimentalApi`, start/resume threads, start/interrupt turns.
- [ ] Parse and emit typed streaming events, detect process exit, restart, and resume registered threads.
- [ ] Test RPC envelopes and event parsing without credentials.

### Task 4: Add conversations and semantic tools

- [ ] Persist lightweight conversation metadata and visible JSONL messages.
- [ ] Register page/database/relation dynamic tools with JSON schemas.
- [ ] Execute read tools immediately and queue dangerous writes for approval.
- [ ] Persist tool summaries, diffs, approvals, and completion states.

### Task 5: Build the Agent conversation interface

- [ ] Add conversation list, scope selector, prompt composer, stop action, stream rendering, tool progress, diff, and approval controls.
- [ ] Resume the selected thread after app/runtime restart.
- [ ] Test state transitions and event projection.

### Task 6: Complete verification

- [ ] Run frontend tests/build and Rust test/fmt/clippy.
- [ ] Exercise database create/edit/filter/sort and relation flows in a temporary vault.
- [ ] Perform a real App Server thread/start, turn/start, streaming, interrupt, restart, and resume probe.
- [ ] Commit each stable subsystem and inspect the clean worktree.
