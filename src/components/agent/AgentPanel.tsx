import { Bot, Check, LoaderCircle, MessageSquarePlus, PanelRightClose, RotateCw, Send, Square, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { ConversationScope } from "../../lib/tauri/conversations";
import { useAgentStore } from "../../stores/agentStore";
import { useCodexStore } from "../../stores/codexStore";
import { usePagesStore } from "../../stores/pagesStore";
import { useWorkspaceStore } from "../../stores/workspaceStore";

function visibleMessages(records: ReturnType<typeof useAgentStore.getState>["records"]) {
  const messages: { role: "user" | "assistant"; text: string }[] = [];
  for (const item of records) {
    if (item.kind === "userMessage") messages.push({ role: "user", text: String(item.data.text ?? "") });
    if (item.kind === "agentMessageDelta") {
      const last = messages[messages.length - 1];
      if (last?.role === "assistant") last.text += String(item.data.delta ?? "");
      else messages.push({ role: "assistant", text: String(item.data.delta ?? "") });
    }
  }
  return messages;
}

export function AgentPanel() {
  const status = useCodexStore((state) => state.status);
  const connect = useCodexStore((state) => state.connect);
  const subscribeStatus = useCodexStore((state) => state.subscribe);
  const toggleAgentPanel = useWorkspaceStore((state) => state.toggleAgentPanel);
  const activePage = usePagesStore((state) => state.activePage);
  const vaultPath = usePagesStore((state) => state.vaultPath);
  const { conversations, activeId, records, plan, diff, tools, approvals, sending, error, initialize, create, open, send, interrupt, resolveApproval } = useAgentStore();
  const [prompt, setPrompt] = useState("");
  const [scopeKind, setScopeKind] = useState<ConversationScope["kind"]>("page");

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void subscribeStatus().then((cleanup) => { if (disposed) cleanup(); else unlisten = cleanup; }).catch(() => undefined);
    void connect();
    return () => { disposed = true; unlisten?.(); };
  }, [connect, subscribeStatus]);

  useEffect(() => {
    if (!vaultPath || status.state !== "ready") return;
    let disposed = false;
    let cleanup: (() => void) | undefined;
    void initialize().then((value) => { if (disposed) value(); else cleanup = value; }).catch(() => undefined);
    return () => { disposed = true; cleanup?.(); };
  }, [initialize, status.state, vaultPath]);

  const scope = useMemo<ConversationScope>(() => {
    if (scopeKind === "database" && activePage?.kind === "database") return { kind: "database", databaseId: activePage.id };
    if (scopeKind === "page" && activePage) return { kind: "page", pageId: activePage.id };
    return { kind: "vault" };
  }, [activePage, scopeKind]);
  const messages = visibleMessages(records);
  const ready = status.state === "ready";
  const submit = () => { const text = prompt.trim(); if (!text || !ready || sending) return; setPrompt(""); void send(text, scope); };

  return <aside className="flex min-h-0 flex-col bg-[var(--qy-sidebar)]" aria-label="Agent 面板">
    <div className="flex h-10 items-center justify-between border-b border-[var(--qy-border)] px-3"><div className="flex items-center gap-2 text-[13px] font-medium"><Bot size={16} />Agent</div><div className="flex items-center gap-1"><button className="icon-button" title="新建对话" aria-label="新建对话" onClick={() => void create(scope)}><MessageSquarePlus size={15} /></button><button className="icon-button" title="关闭 Agent 面板" aria-label="关闭 Agent 面板" onClick={toggleAgentPanel}><PanelRightClose size={16} /></button></div></div>
    <div className="flex items-center gap-1 overflow-x-auto border-b border-[var(--qy-border)] px-2 py-1">{conversations.map((conversation) => <button key={conversation.id} type="button" onClick={() => void open(conversation.id)} className={`max-w-32 shrink-0 truncate rounded-md px-2 py-1 text-[11px] ${conversation.id === activeId ? "bg-white text-[var(--qy-ink)]" : "text-[var(--qy-muted)] hover:bg-white"}`}>{conversation.title}</button>)}</div>
    <div className="min-h-0 flex-1 overflow-y-auto px-3 py-3">
      {!ready && <div className="flex h-full flex-col items-center justify-center text-center text-[13px] text-[var(--qy-muted)]">{status.state === "starting" ? <LoaderCircle className="mb-3 animate-spin" size={22} /> : <Bot className="mb-3 text-[var(--qy-faint)]" size={24} />}<p>{status.state === "failed" ? status.message : status.state === "starting" ? "正在连接本机 Codex" : "Codex 未连接"}</p>{status.state === "failed" && <button className="mt-3 inline-flex items-center gap-1 text-[var(--qy-accent)]" onClick={() => void connect()}><RotateCw size={13}/>重新连接</button>}</div>}
      {ready && messages.length === 0 && <div className="grid h-full place-items-center text-center text-[12px] text-[var(--qy-faint)]"><div><Bot className="mx-auto mb-2" size={22}/><p>选择上下文后开始对话</p></div></div>}
      {ready && <div className="space-y-3">{messages.map((message, index) => <div key={`${message.role}-${index}`} className={message.role === "user" ? "ml-8 rounded-md bg-white px-3 py-2 text-[12px]" : "whitespace-pre-wrap px-1 text-[12px] leading-5 text-[var(--qy-ink)]"}>{message.text}</div>)}{sending && <div className="flex items-center gap-2 text-[11px] text-[var(--qy-faint)]"><LoaderCircle className="animate-spin" size={13}/>Agent 正在工作</div>}</div>}
      {tools.length > 0 && <details className="mt-3 border-t border-[var(--qy-border)] pt-2"><summary className="cursor-pointer text-[11px] text-[var(--qy-muted)]">工具进度 · {tools.length}</summary>{tools.map((tool) => <div key={tool.id} className="py-1 text-[11px] text-[var(--qy-faint)]">{tool.namespace ? `${tool.namespace}.` : ""}{tool.tool ?? tool.itemType} · {tool.status ?? "进行中"}</div>)}</details>}
      {plan != null && <details className="mt-3 border-t border-[var(--qy-border)] pt-2"><summary className="cursor-pointer text-[11px] text-[var(--qy-muted)]">执行计划</summary><pre className="mt-2 whitespace-pre-wrap text-[10px] text-[var(--qy-faint)]">{JSON.stringify(plan, null, 2)}</pre></details>}
      {diff && <details className="mt-3 border-t border-[var(--qy-border)] pt-2"><summary className="cursor-pointer text-[11px] text-[var(--qy-muted)]">变更 diff</summary><pre className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap bg-white p-2 text-[10px]">{diff}</pre></details>}
      {approvals.map((approval) => <div key={approval.id} className="mt-3 border-t border-[var(--qy-border)] pt-2"><div className="text-[12px] font-medium">等待审批：{approval.namespace}.{approval.tool}</div><pre className="my-2 max-h-32 overflow-auto whitespace-pre-wrap bg-white p-2 text-[10px]">{approval.diff}</pre><div className="flex justify-end gap-1"><button className="inline-flex h-7 items-center gap-1 rounded-md px-2 text-[11px] text-red-600 hover:bg-white" onClick={() => void resolveApproval(approval.id, false)}><X size={13}/>拒绝</button><button className="inline-flex h-7 items-center gap-1 rounded-md bg-[var(--qy-accent)] px-2 text-[11px] text-white" onClick={() => void resolveApproval(approval.id, true)}><Check size={13}/>批准</button></div></div>)}
      {error && <div className="mt-3 text-[11px] text-red-600">{error}</div>}
    </div>
    <div className="border-t border-[var(--qy-border)] p-3"><div className="mb-2 flex items-center justify-between"><select value={scopeKind} onChange={(event) => setScopeKind(event.target.value as ConversationScope["kind"])} className="bg-transparent text-[11px] text-[var(--qy-muted)]"><option value="page" disabled={!activePage}>当前页面</option><option value="database" disabled={activePage?.kind !== "database"}>当前数据库</option><option value="vault">整个笔记库</option></select>{sending && <button className="icon-button" title="停止" aria-label="停止 Agent" onClick={() => void interrupt()}><Square size={13}/></button>}</div><div className="flex items-end gap-1 rounded-md border border-[var(--qy-border)] bg-white p-2"><textarea value={prompt} onChange={(event) => setPrompt(event.target.value)} onKeyDown={(event) => { if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); submit(); } }} disabled={!ready} className="h-16 min-w-0 flex-1 resize-none text-[12px] outline-none" placeholder="让 Agent 读取或修改笔记"/><button className="icon-button shrink-0" title="发送" aria-label="发送消息" disabled={!prompt.trim() || !ready || sending} onClick={submit}><Send size={15}/></button></div></div>
  </aside>;
}
