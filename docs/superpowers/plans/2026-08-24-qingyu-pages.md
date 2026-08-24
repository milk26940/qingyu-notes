# QingYu Pages Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build the local-first Markdown page system for QingYu with stable UUIDs, hierarchy, links, search, trash, and basic history.

**Architecture:** Rust owns vault files, atomic writes, metadata parsing, hierarchy, link extraction, search projection, trash, and history through typed Tauri commands. React owns page-tree/editor interaction and calls those commands through a typed adapter and Zustand store.

**Tech Stack:** Rust, Tokio, serde/serde_json, Tauri 2 commands; React 18, TypeScript, Zustand, CodeMirror 6, Vitest.

---

### Task 1: Add the Rust page domain and filesystem repository

Create `src-tauri/src/pages/model.rs`, `repository.rs`, `mod.rs`; modify `src-tauri/Cargo.toml` and `src-tauri/src/lib.rs`.

- [ ] Add serializable `Page`, `PageSummary`, `PageLink`, `PageHistoryEntry`, `SearchResult` and `PageKind` types.
- [ ] Implement vault initialization with default Documents path, `.noteapp`, and safe filename generation.
- [ ] Implement frontmatter parse/serialize and atomic Markdown writes.
- [ ] Add tests for UUID round-trip, unsafe filename handling, and parent-child listing.

### Task 2: Add typed page commands

Create `src-tauri/src/commands/pages.rs`; modify command registration.

- [ ] Expose vault init/list/get/create/update/move/trash/restore/history/search/links commands.
- [ ] Validate IDs, prevent moving a page below itself, and return structured `AppError` values.
- [ ] Record a JSONL history entry before every successful update, move, trash, or restore.
- [ ] Add Rust tests for link extraction, reverse links, search snippets, trash restore, and history ordering.

### Task 3: Add the frontend page adapter and Zustand store

Create `src/lib/tauri/pages.ts`, `src/stores/pagesStore.ts`, and `src/stores/pagesStore.test.ts`.

- [ ] Define typed wrappers for every page command.
- [ ] Store vault path, page tree, active page, editor buffer, save state, search query/results, links, and trash entries.
- [ ] Implement create, save (debounced at component boundary), move, trash, restore, refresh, and search actions.
- [ ] Test command sequencing and save-state transitions with mocked adapter functions.

### Task 4: Replace the phase-one shell with the page workspace

Modify `src/app/App.tsx`, `src/components/layout/Sidebar.tsx`, `src/components/editor/MarkdownEditor.tsx`, `src/styles/index.css`; create `src/components/pages/PageTree.tsx` and `PageToolbar.tsx`.

- [ ] Render a recursive expandable tree with stable draggable nodes and drop targets.
- [ ] Add new-page, save indicator, trash and search controls.
- [ ] Open active pages in CodeMirror, edit title/content, and save through the store.
- [ ] Render backlinks and search results with clickable navigation.
- [ ] Add focused React tests for selection, creation, search and save state.

### Task 5: Verify and commit the second phase

- [ ] Run `npm test`, `npm run build`, `cargo test`, `cargo fmt -- --check`, and `cargo clippy -- -D warnings`.
- [ ] Start the Tauri app against a temporary vault, create/edit/move/search/trash/restore a page, and inspect resulting files.
- [ ] Commit implementation and documentation with a phase-two commit.
