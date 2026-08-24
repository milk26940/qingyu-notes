# QingYu Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a runnable Windows Tauri 2 application named QingYu with the approved three-pane React shell, a CodeMirror 6 editor shell, and a live connection to the user's existing local `codex app-server`.

**Architecture:** React owns presentation and transient Zustand state; Tauri commands own native capabilities. A focused Rust `codex` module locates the installed Codex executable, starts App Server over stdio, performs the JSON-RPC initialization handshake, emits lifecycle events, and shuts the child down cleanly. This phase intentionally contains no note persistence, database-note behavior, Agent write tools, or backup implementation.

**Tech Stack:** React 18, TypeScript, Vite, TailwindCSS 4, Radix UI, CodeMirror 6, Zustand, Tauri 2, Rust, Tokio, serde, serde_json, Vitest.

---

## Scope And File Map

Create these focused units:

- `package.json`: exact frontend scripts and dependency versions.
- `vite.config.ts`: Vite, React, Tailwind and test configuration.
- `src/app/App.tsx`: compose the application shell only.
- `src/components/layout/WorkspaceShell.tsx`: stable three-pane desktop layout.
- `src/components/editor/MarkdownEditor.tsx`: CodeMirror lifecycle boundary.
- `src/components/agent/AgentPanel.tsx`: connection status and disabled prompt shell.
- `src/stores/workspaceStore.ts`: sidebar and Agent-panel UI state.
- `src/stores/codexStore.ts`: native Codex status and event subscription.
- `src/lib/tauri/codex.ts`: typed Tauri command adapter.
- `src-tauri/src/codex/command.rs`: locate a launchable local Codex command.
- `src-tauri/src/codex/protocol.rs`: minimal JSON-RPC handshake types and parsing.
- `src-tauri/src/codex/runtime.rs`: child-process lifecycle, handshake and event forwarding.
- `src-tauri/src/commands/codex.rs`: thin Tauri command boundary.
- `src-tauri/src/error.rs`: serializable native error envelope.

Do not create note domain models in this plan. Do not read `auth.json`, `config.toml`, CCSwitch SQLite, or Codex SQLite directly.

### Task 1: Install And Verify The Windows Build Prerequisites

**Files:**
- No repository files

- [ ] **Step 1: Verify the already available tools**

Run:

```powershell
node --version
npm --version
codex --version
codex login status
```

Expected: Node 22.x, npm 10.x, Codex 0.146.0 or newer, and a successful login status. Treat version values as observed prerequisites, not application constants.

- [ ] **Step 2: Install the MSVC build tools when `where.exe link.exe` has no result**

Run in an elevated PowerShell only when the linker is absent:

```powershell
$installer = Join-Path $env:TEMP 'vs_BuildTools.exe'
Invoke-WebRequest 'https://aka.ms/vs/17/release/vs_BuildTools.exe' -OutFile $installer
Start-Process -FilePath $installer -ArgumentList '--quiet','--wait','--norestart','--add','Microsoft.VisualStudio.Workload.VCTools','--includeRecommended' -Wait
```

Expected: Visual Studio Build Tools installs the MSVC compiler, Windows SDK and linker. A restart may be required before continuing.

- [ ] **Step 3: Install Rust with rustup when `cargo` is absent**

Run:

```powershell
$rustup = Join-Path $env:TEMP 'rustup-init.exe'
Invoke-WebRequest 'https://win.rustup.rs/x86_64' -OutFile $rustup
& $rustup -y --default-toolchain stable --default-host x86_64-pc-windows-msvc
```

Open a fresh PowerShell, then run:

```powershell
rustc --version
cargo --version
rustup show active-toolchain
```

Expected: stable `x86_64-pc-windows-msvc` toolchain is active.

- [ ] **Step 4: Verify WebView2**

Run:

```powershell
Get-ItemProperty 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\*' -ErrorAction SilentlyContinue |
  Where-Object { $_.name -match 'WebView2' } |
  Select-Object name,pv
```

Expected: Microsoft Edge WebView2 Runtime is present. Install the Evergreen Runtime from Microsoft only when it is absent.

### Task 2: Scaffold And Pin The Tauri Application

**Files:**
- Create: `package.json`
- Create: `package-lock.json`
- Create: `index.html`
- Create: `tsconfig.json`
- Create: `tsconfig.node.json`
- Create: `vite.config.ts`
- Create: `src/main.tsx`
- Create: `src/app/App.tsx`
- Create: `src/styles/index.css`
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/build.rs`
- Create: `src-tauri/tauri.conf.json`
- Create: `src-tauri/capabilities/default.json`
- Create: `src-tauri/src/main.rs`
- Create: `src-tauri/src/lib.rs`
- Create: `.gitignore`

- [ ] **Step 1: Scaffold React TypeScript and Tauri 2 into the repository root**

Run from `D:\笔记`:

```powershell
npx --yes create-tauri-app@latest . --manager npm --template react-ts --tauri-version 2 --identifier com.qingyu.notes --force --yes
```

Expected: the scaffold is added without deleting `docs/`. Immediately run `git status --short` and stop if the tracked design document is removed or modified.

- [ ] **Step 2: Pin React 18 and install the phase-one dependencies**

Run:

```powershell
npm install react@18.3.1 react-dom@18.3.1 zustand@5.0.15 @radix-ui/react-scroll-area @radix-ui/react-tooltip @radix-ui/react-separator @radix-ui/react-collapsible lucide-react @codemirror/state @codemirror/view @codemirror/commands @codemirror/lang-markdown @codemirror/language @codemirror/theme-one-dark
npm install --save-dev @types/react@18 @types/react-dom@18 tailwindcss@4.3.3 @tailwindcss/vite@4.3.3 vitest jsdom @testing-library/react @testing-library/jest-dom
```

Expected: `package-lock.json` is updated and only one React 18 version is installed.

- [ ] **Step 3: Set the exact scripts in `package.json`**

```json
{
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "test": "vitest run",
    "test:watch": "vitest",
    "tauri": "tauri"
  }
}
```

Preserve dependencies generated or installed in the previous steps.

- [ ] **Step 4: Configure Vite, Tailwind and Vitest in `vite.config.ts`**

```ts
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
  },
});
```

- [ ] **Step 5: Set application identity in `src-tauri/tauri.conf.json`**

Ensure these values are present while preserving scaffolded build and bundle keys:

```json
{
  "productName": "轻羽",
  "version": "0.1.0",
  "identifier": "com.qingyu.notes",
  "app": {
    "windows": [
      {
        "title": "轻羽",
        "width": 1280,
        "height": 800,
        "minWidth": 960,
        "minHeight": 640
      }
    ]
  }
}
```

- [ ] **Step 6: Ignore generated and brainstorming files**

Add to `.gitignore`:

```gitignore
node_modules/
dist/
src-tauri/target/
.superpowers/
*.log
```

- [ ] **Step 7: Verify the frontend scaffold**

Run:

```powershell
npm run build
```

Expected: TypeScript and Vite build complete successfully.

- [ ] **Step 8: Commit the scaffold**

```powershell
git add .gitignore package.json package-lock.json index.html tsconfig*.json vite.config.ts src src-tauri
git commit -m "chore: scaffold QingYu Tauri application"
```

### Task 3: Add The Native Error And Codex Protocol Boundaries

**Files:**
- Create: `src-tauri/src/error.rs`
- Create: `src-tauri/src/codex/mod.rs`
- Create: `src-tauri/src/codex/protocol.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write protocol parsing tests in `src-tauri/src/codex/protocol.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_initialize_response() {
        let line = r#"{"id":0,"result":{"userAgent":"Codex Desktop/0.146.0","codexHome":"C:\\Users\\26940\\.codex","platformFamily":"windows","platformOs":"windows"}}"#;
        let response = parse_initialize_response(line).unwrap();
        assert_eq!(response.codex_home, r"C:\Users\26940\.codex");
        assert_eq!(response.platform_family, "windows");
    }

    #[test]
    fn rejects_a_non_handshake_message() {
        let error = parse_initialize_response(r#"{"method":"turn/started","params":{}}"#).unwrap_err();
        assert_eq!(error.code, "codex_invalid_handshake");
    }
}
```

- [ ] **Step 2: Run the focused Rust test and verify failure**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml codex::protocol::tests -- --nocapture
```

Expected: FAIL because `parse_initialize_response` and its types do not exist.

- [ ] **Step 3: Implement `src-tauri/src/error.rs`**

```rust
use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: String,
    pub message: String,
}

impl AppError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self { code: code.into(), message: message.into() }
    }
}
```

- [ ] **Step 4: Implement the minimum handshake parser**

Create serde wire types private to `protocol.rs`, expose a `CodexInitializeResult` with camel-case serialization, and implement:

```rust
pub fn parse_initialize_response(line: &str) -> Result<CodexInitializeResult, AppError> {
    let response: InitializeResponse = serde_json::from_str(line)
        .map_err(|error| AppError::new("codex_invalid_json", error.to_string()))?;
    if response.id != 0 {
        return Err(AppError::new("codex_invalid_handshake", "Codex returned an unexpected response id"));
    }
    response.result.ok_or_else(|| {
        AppError::new("codex_invalid_handshake", "Codex did not return initialization data")
    })
}
```

The public result fields are `user_agent`, `codex_home`, `platform_family`, and `platform_os` with `#[serde(rename_all = "camelCase")]`.

- [ ] **Step 5: Export modules and run tests**

Add to `src-tauri/src/lib.rs`:

```rust
mod codex;
mod error;
```

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml codex::protocol::tests
```

Expected: 2 passed.

- [ ] **Step 6: Commit**

```powershell
git add src-tauri/src/error.rs src-tauri/src/codex src-tauri/src/lib.rs
git commit -m "feat: add Codex protocol boundary"
```

### Task 4: Implement The Local Codex Runtime

**Files:**
- Create: `src-tauri/src/codex/command.rs`
- Create: `src-tauri/src/codex/runtime.rs`
- Create: `src-tauri/src/commands/mod.rs`
- Create: `src-tauri/src/commands/codex.rs`
- Modify: `src-tauri/src/codex/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add the native dependencies**

Ensure `src-tauri/Cargo.toml` contains:

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tauri = { version = "2", features = [] }
tokio = { version = "1", features = ["io-util", "process", "sync", "time"] }
```

Keep scaffolded dependencies such as `tauri-plugin-opener` when present.

- [ ] **Step 2: Write command-resolution tests**

In `command.rs`, test that an `.exe` path creates a direct command and a `.cmd` path creates a `cmd.exe /D /S /C` launch description. Use fixed fake paths; do not depend on the developer machine PATH.

```rust
#[test]
fn exe_candidate_is_launched_directly() {
    let launch = CodexLaunch::from_path(PathBuf::from(r"C:\tools\codex.exe"));
    assert_eq!(launch.program, PathBuf::from(r"C:\tools\codex.exe"));
    assert_eq!(launch.prefix_args, Vec::<String>::new());
}

#[test]
fn cmd_candidate_uses_cmd_exe() {
    let launch = CodexLaunch::from_path(PathBuf::from(r"C:\tools\codex.cmd"));
    assert_eq!(launch.program, PathBuf::from("cmd.exe"));
    assert_eq!(launch.prefix_args[0..3], ["/D", "/S", "/C"]);
}
```

- [ ] **Step 3: Implement Codex discovery**

`resolve_codex_launch()` must:

1. Run `where.exe codex.exe` and use the first existing file.
2. Fall back to `where.exe codex.cmd`.
3. Return `AppError { code: "codex_not_found", ... }` if neither resolves.

Never hard-code the current npm vendor path.

- [ ] **Step 4: Implement runtime state and status**

Expose these types from `runtime.rs`:

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum CodexStatus {
    Stopped,
    Starting,
    Ready { user_agent: String, codex_home: String },
    Failed { code: String, message: String },
}

pub struct CodexRuntime {
    status: tokio::sync::RwLock<CodexStatus>,
    process: tokio::sync::Mutex<Option<CodexProcess>>,
}
```

`CodexProcess` owns the child, stdin and stdout reader task. `start()` must spawn:

```text
codex app-server --listen stdio://
```

It then writes exactly two newline-terminated messages:

```json
{"method":"initialize","id":0,"params":{"clientInfo":{"name":"qingyu_notes","title":"轻羽","version":"0.1.0"}}}
{"method":"initialized","params":{}}
```

Wait at most 10 seconds for response `id: 0`, parse it with `parse_initialize_response`, set status to `Ready`, and emit `codex://status` through `AppHandle::emit`.

- [ ] **Step 5: Implement clean shutdown and event forwarding**

- Forward subsequent valid JSON lines as `codex://event` payloads.
- Forward malformed stdout lines as a structured warning event without logging secrets.
- `stop()` closes stdin, kills the owned child when still running, waits for exit, aborts the reader task, and sets `Stopped`.
- Register an application exit handler that calls the same shutdown path.

- [ ] **Step 6: Add thin Tauri commands**

In `commands/codex.rs` expose:

```rust
#[tauri::command]
pub async fn codex_status(runtime: State<'_, CodexRuntime>) -> Result<CodexStatus, AppError>;

#[tauri::command]
pub async fn codex_start(app: AppHandle, runtime: State<'_, CodexRuntime>) -> Result<CodexStatus, AppError>;

#[tauri::command]
pub async fn codex_stop(app: AppHandle, runtime: State<'_, CodexRuntime>) -> Result<CodexStatus, AppError>;
```

Register `CodexRuntime::default()` as managed state and all three commands in `src-tauri/src/lib.rs`.

- [ ] **Step 7: Run focused and full native tests**

Run:

```powershell
cargo test --manifest-path src-tauri/Cargo.toml codex::command::tests
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: all tests pass.

- [ ] **Step 8: Commit**

```powershell
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src
git commit -m "feat: manage local Codex app server"
```

### Task 5: Build The Typed Frontend Codex Store

**Files:**
- Create: `src/lib/tauri/codex.ts`
- Create: `src/stores/codexStore.ts`
- Create: `src/stores/codexStore.test.ts`
- Create: `src/test/setup.ts`

- [ ] **Step 1: Configure the test setup**

```ts
import "@testing-library/jest-dom/vitest";
```

- [ ] **Step 2: Write the failing store test**

Test that `connect()` transitions from `starting` to the resolved native `ready` status and stores `codexHome`. Mock only `startCodex`; do not mock Zustand internals.

```ts
expect(useCodexStore.getState().status.state).toBe("stopped");
await useCodexStore.getState().connect();
expect(useCodexStore.getState().status).toEqual({
  state: "ready",
  userAgent: "Codex Desktop/0.146.0",
  codexHome: "C:\\Users\\26940\\.codex",
});
```

- [ ] **Step 3: Verify the test fails**

Run:

```powershell
npm test -- src/stores/codexStore.test.ts
```

Expected: FAIL because the adapter and store do not exist.

- [ ] **Step 4: Implement the typed adapter**

Define the discriminated union:

```ts
export type CodexStatus =
  | { state: "stopped" }
  | { state: "starting" }
  | { state: "ready"; userAgent: string; codexHome: string }
  | { state: "failed"; code: string; message: string };
```

Wrap only `invoke("codex_status")`, `invoke("codex_start")`, `invoke("codex_stop")`, plus `listen<CodexStatus>("codex://status", ...)`. No component imports Tauri APIs directly.

- [ ] **Step 5: Implement `useCodexStore`**

State contains `status`, `connect`, `disconnect`, and `subscribe`. `connect()` sets `starting`, calls the adapter, and converts rejected `AppError` values into `failed`. `subscribe()` returns the Tauri unlisten function.

- [ ] **Step 6: Run the test and commit**

Run:

```powershell
npm test -- src/stores/codexStore.test.ts
```

Expected: PASS.

Commit:

```powershell
git add src/lib src/stores src/test
git commit -m "feat: add frontend Codex connection store"
```

### Task 6: Build The QingYu Workspace Shell

**Files:**
- Create: `src/components/layout/WorkspaceShell.tsx`
- Create: `src/components/layout/Sidebar.tsx`
- Create: `src/components/layout/Topbar.tsx`
- Create: `src/components/layout/StatusBar.tsx`
- Create: `src/components/agent/AgentPanel.tsx`
- Create: `src/stores/workspaceStore.ts`
- Modify: `src/app/App.tsx`
- Modify: `src/styles/index.css`

- [ ] **Step 1: Implement stable layout state**

`workspaceStore.ts` contains only `sidebarOpen`, `agentPanelOpen`, `sidebarWidth`, `agentPanelWidth` and explicit toggle/set actions. Use bounded widths: sidebar 220-360 px and Agent panel 300-520 px.

- [ ] **Step 2: Implement the layout components**

Use semantic full-height regions:

```tsx
<div className="grid h-screen grid-rows-[40px_minmax(0,1fr)_24px] overflow-hidden">
  <Topbar />
  <div className="grid min-h-0" style={{ gridTemplateColumns }}>
    {sidebarOpen && <Sidebar />}
    <main className="min-w-0 overflow-hidden">{children}</main>
    {agentPanelOpen && <AgentPanel />}
  </div>
  <StatusBar />
</div>
```

The sidebar contains compact static placeholders for Search, Favorites, Pages, Databases and Trash. These controls are disabled or inert in phase one; do not create fake persistence.

- [ ] **Step 3: Implement the Agent status panel**

On mount, subscribe to native status and call `connect()`. Show:

- `正在连接本机 Codex`
- `Codex 已连接`
- `未找到本机 Codex`
- `Codex 连接失败`

When ready, show `codexHome` as secondary diagnostic text. The prompt box is disabled with placeholder `页面系统完成后即可开始对话`.

- [ ] **Step 4: Apply the restrained visual system**

In `src/styles/index.css`, import Tailwind and define neutral CSS variables with one blue action accent, green healthy status, amber warning and red destructive status. Use 0-8 px radii, 13-15 px interface text, no gradients, no decorative floating cards, and no negative letter spacing.

- [ ] **Step 5: Verify the production frontend build**

Run:

```powershell
npm run build
```

Expected: successful TypeScript and Vite build.

- [ ] **Step 6: Commit**

```powershell
git add src/app src/components src/stores/workspaceStore.ts src/styles
git commit -m "feat: add QingYu workspace shell"
```

### Task 7: Add The CodeMirror Markdown Editor Shell

**Files:**
- Create: `src/components/editor/MarkdownEditor.tsx`
- Create: `src/components/editor/editorExtensions.ts`
- Create: `src/components/editor/MarkdownEditor.test.tsx`
- Modify: `src/app/App.tsx`

- [ ] **Step 1: Write the editor lifecycle test**

Render the editor with a short Markdown value, rerender with a new external value, and verify the CodeMirror document changes without creating a second editor root. Unmount and verify the root is empty.

- [ ] **Step 2: Run the focused test and verify failure**

```powershell
npm test -- src/components/editor/MarkdownEditor.test.tsx
```

Expected: FAIL because `MarkdownEditor` does not exist.

- [ ] **Step 3: Define editor extensions**

`editorExtensions.ts` returns an extension list containing Markdown language support, history, default keymap, history keymap, line wrapping, editable state, update listener, and a light theme using the QingYu CSS variables.

- [ ] **Step 4: Implement one CodeMirror instance per mount**

`MarkdownEditor` accepts:

```ts
type MarkdownEditorProps = {
  value: string;
  onChange: (value: string) => void;
  readOnly?: boolean;
};
```

Create `EditorState` and `EditorView` in one mount-only effect, destroy the view on cleanup, and use a second effect to dispatch an external-value replacement only when the document differs. Prevent the replacement transaction from echoing a duplicate `onChange`.

- [ ] **Step 5: Mount the editor with explicit placeholder content**

In `App.tsx`, keep the phase-one document in component state:

```md
# 欢迎使用轻羽

这是技术骨架预览。页面保存将在下一阶段接入。
```

This is intentionally not persisted.

- [ ] **Step 6: Run tests and build**

```powershell
npm test -- src/components/editor/MarkdownEditor.test.tsx
npm run build
```

Expected: focused test and production build pass.

- [ ] **Step 7: Commit**

```powershell
git add src/components/editor src/app/App.tsx
git commit -m "feat: add CodeMirror markdown editor shell"
```

### Task 8: Verify The Desktop Integration End To End

**Files:**
- Modify only files required by defects found in this task

- [ ] **Step 1: Run all automated checks**

```powershell
npm test
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

Expected: all commands exit 0.

- [ ] **Step 2: Start the Tauri development application**

```powershell
npm run tauri dev
```

Expected: a Windows window titled `轻羽` opens, renders the three-pane layout, and reports `Codex 已连接` with the existing `C:\Users\26940\.codex` home.

- [ ] **Step 3: Verify the child-process lifecycle**

Close the QingYu window, then run:

```powershell
Get-CimInstance Win32_Process |
  Where-Object { $_.Name -eq 'codex.exe' -and $_.CommandLine -match 'app-server' } |
  Select-Object ProcessId,CommandLine
```

Expected: no App Server process launched by QingYu remains. Existing unrelated Codex processes must not be terminated.

- [ ] **Step 4: Verify graceful missing-Codex behavior**

Temporarily start Tauri from a PowerShell process whose PATH omits the Codex npm and vendor directories. Do not rename or delete the user's Codex installation.

Expected: QingYu remains usable, the Agent panel reports `未找到本机 Codex`, and the Markdown editor still works.

- [ ] **Step 5: Build the Windows development bundle**

```powershell
npm run tauri build -- --debug
```

Expected: Tauri produces a debug Windows bundle without embedding or copying `C:\Users\26940\.codex`.

- [ ] **Step 6: Inspect the final diff and commit fixes**

```powershell
git status --short
git diff --check
git diff --stat HEAD
```

Commit only necessary verification fixes:

```powershell
$verifiedFiles = git diff --name-only
$verifiedFiles | ForEach-Object { git add -- $_ }
git commit -m "fix: complete QingYu foundation verification"
```

Skip this commit when verification required no changes.

## Completion Criteria

- The Tauri window opens on Windows and remains usable when Codex is unavailable.
- The approved three-pane QingYu shell renders without overlap at the minimum window size.
- CodeMirror owns exactly one editor instance and edits the phase-one in-memory Markdown.
- The Rust backend locates the installed Codex without a hard-coded vendor path.
- App Server performs a real initialization handshake using the existing `CODEX_HOME` and login.
- The UI displays live connection state.
- Closing QingYu terminates only the child App Server process it owns.
- No code reads or copies `auth.json`, Codex SQLite, or CCSwitch SQLite.
- Frontend tests, frontend build, Rust tests, formatting and Clippy pass.
