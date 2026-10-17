import {
  Bot,
  Check,
  ChevronDown,
  FileText,
  History,
  LoaderCircle,
  MessageSquarePlus,
  PanelRightClose,
  RotateCw,
  Send,
  Sparkles,
  Square,
  X,
} from "lucide-react";
import { useShallow } from "zustand/react/shallow";
import { createPortal } from "react-dom";
import { memo, useEffect, useMemo, useRef, useState } from "react";
import type { ConversationScope } from "../../lib/tauri/conversations";
import type { ConversationAttachment } from "../../lib/tauri/conversations";
import { clipboardImageFiles } from "../editor/imageAttachments";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useAgentStore } from "../../stores/agentStore";
import { useCodexStore } from "../../stores/codexStore";
import { usePagesStore } from "../../stores/pagesStore";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { invoke } from "@tauri-apps/api/core";
import { AttachmentPreview } from "./AgentContent";
import { conversationTimeline, requestText } from "./conversationTimeline";
import { ConversationTurn } from "./ConversationTurn";
import { MessageQueue } from "./MessageQueue";

function scopeLabel(scope: ConversationScope) {
  if (scope.kind === "page") return "当前页面";
  if (scope.kind === "database") return "当前数据库";
  return "整个笔记库";
}

function conversationTime(value: string) {
  return new Date(Number(value) * 1000).toLocaleString("zh-CN", {
    month: "numeric",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export const AgentPanel = memo(function AgentPanel({
  selectedText = "",
  onClearSelection,
}: {
  selectedText?: string;
  onClearSelection?: () => void;
}) {
  const status = useCodexStore((state) => state.status);
  const connect = useCodexStore((state) => state.connect);
  const models = useCodexStore((state) => state.models) ?? [];
  const modelsLoading = useCodexStore((state) => state.modelsLoading) ?? false;
  const modelsError = useCodexStore((state) => state.modelsError) ?? null;
  const loadModels = useCodexStore((state) => state.loadModels);
  const toggleAgentPanel = useWorkspaceStore((state) => state.toggleAgentPanel);
  const activePage = usePagesStore(useShallow((state) => state.activePage ? { id: state.activePage.id, title: state.activePage.title, kind: state.activePage.kind } : null));
  const openPage = usePagesStore((state) => state.open);
  const reloadPage = usePagesStore((state) => state.reload);
  const storeSelectedModel = useAgentStore((state) => state?.selectedModel);
  const selectedModel = typeof storeSelectedModel === "string" ? storeSelectedModel : null;
  const setSelectedModel = useAgentStore((state) => state?.setSelectedModel) ?? (() => undefined);
  const {
    conversations,
    activeId,
    records,
    queue,
    queuePaused,
    enqueue,
    editQueued,
    removeQueued,
    resumeQueue,
    approvals,
    sending,
    error,
    create,
    open,
    send,
    uploadAttachments,
    uploadAttachmentPaths,
    interrupt,
    resolveApproval,
  } = useAgentStore();
  const [prompt, setPrompt] = useState("");
  const promptRef = useRef<HTMLTextAreaElement>(null);
  const attachmentInputRef = useRef<HTMLInputElement>(null);
  const inputZoneRef = useRef<HTMLDivElement>(null);
  const modelMenuRef = useRef<HTMLDivElement>(null);
  const modelOptionsRef = useRef<HTMLDivElement>(null);
  const [modelPosition, setModelPosition] = useState({ left: 0, bottom: 0, width: 280, maxHeight: 280 });
  const [modelMenuOpen, setModelMenuOpen] = useState(false);
  const [attachments, setAttachments] = useState<ConversationAttachment[]>([]);
  const [dragging, setDragging] = useState(false);
  const [uploading, setUploading] = useState(false);
  const previousSending = useRef(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  useEffect(() => {
    localStorage.setItem("qingyu-agent-mode", "auto");
  }, []);
  useEffect(() => {
    if (previousSending.current && !sending && activePage && reloadPage) {
      void reloadPage(activePage.id);
    }
    previousSending.current = sending;
  }, [activePage?.id, reloadPage, sending]);
  useEffect(() => {
    approvals.forEach((approval) => {
        void resolveApproval(approval.id, true);
      });
  }, [approvals, resolveApproval]);

  const scope = useMemo<ConversationScope>(
    () =>
      activePage ? { kind: "page", pageId: activePage.id } : { kind: "vault" },
    [activePage],
  );
  const activeConversation = conversations.find((item) => item.id === activeId);
  const turns = useMemo(() => conversationTimeline(records, activeConversation?.activeTurnId ?? null, sending), [records, activeConversation?.activeTurnId, sending]);
  const [visibleCount, setVisibleCount] = useState(20);
  const [expandedTurns, setExpandedTurns] = useState<Record<string, boolean>>({});
  const [showLatest, setShowLatest] = useState(false);
  const [includePage, setIncludePage] = useState(true);
  const firstVisible = Math.max(0, turns.length - visibleCount);
  const queued = activeId ? queue?.[activeId] ?? [] : [];
  const paused = activeId ? queuePaused?.[activeId] ?? false : false;
  useEffect(() => { setIncludePage(true); }, [activePage?.id]);
  useEffect(() => { if (promptRef.current) { promptRef.current.style.height = "auto"; promptRef.current.style.height = Math.min(180, promptRef.current.scrollHeight) + "px"; } }, [prompt]);
  const messageViewport = useRef<HTMLDivElement>(null);
  const followReply = useRef(true);
  useEffect(() => { setVisibleCount(20); setExpandedTurns({}); setShowLatest(false); followReply.current = true; }, [activeId]);
  useEffect(() => {
    if (!followReply.current || !messageViewport.current) return;
    const frame = requestAnimationFrame(() => { const element = messageViewport.current; if (element) element.scrollTop = element.scrollHeight; });
    return () => cancelAnimationFrame(frame);
  }, [turns, sending]);
  const sortedConversations = useMemo(
    () =>
      [...conversations].sort(
        (left, right) => Number(right.updatedAt) - Number(left.updatedAt),
      ),
    [conversations],
  );
  const ready = status.state === "ready";

  useEffect(() => {
    if (ready && models.length === 0 && !modelsLoading && !modelsError && loadModels) {
      void loadModels();
    }
  }, [ready, models.length, modelsLoading, modelsError, loadModels]);

  useEffect(() => {
    if (sending) {
      setModelMenuOpen(false);
    }
  }, [sending]);

  useEffect(() => {
    if (!modelMenuOpen) return;
    const positionMenu = () => {
      const rect = modelMenuRef.current?.getBoundingClientRect();
      if (!rect) return;
      const width = Math.min(285, window.innerWidth - 32);
      setModelPosition({ left: Math.max(16, Math.min(rect.left, window.innerWidth - width - 16)), bottom: window.innerHeight - rect.top + 8, width, maxHeight: Math.max(80, Math.min(280, rect.top - 24)) });
    };
    positionMenu();
    const handleClickOutside = (event: MouseEvent) => {
      if (modelMenuRef.current && !modelMenuRef.current.contains(event.target as Node) && !modelOptionsRef.current?.contains(event.target as Node)) {
        setModelMenuOpen(false);
      }
    };
    const escape = (event: KeyboardEvent) => { if (event.key === "Escape") setModelMenuOpen(false); };
    window.addEventListener("mousedown", handleClickOutside);
    window.addEventListener("resize", positionMenu);
    window.addEventListener("scroll", positionMenu, true);
    window.addEventListener("keydown", escape);
    return () => { window.removeEventListener("mousedown", handleClickOutside); window.removeEventListener("resize", positionMenu); window.removeEventListener("scroll", positionMenu, true); window.removeEventListener("keydown", escape); };
  }, [modelMenuOpen]);

  const defaultModel = useMemo(() => models.find((item) => item.isDefault), [models]);
  const selectedModelOption = useMemo(
    () => (selectedModel ? models.find((item) => item.model === selectedModel) : null),
    [models, selectedModel],
  );
  const modelButtonLabel = useMemo(() => {
    if (selectedModel) {
      return selectedModelOption?.displayName || selectedModel;
    }
    if (defaultModel) {
      return `跟随 Codex (${defaultModel.displayName})`;
    }
    return "跟随 Codex";
  }, [selectedModel, selectedModelOption, defaultModel]);

  const addFiles = async (files: File[]) => {
    if (!files.length || uploading) return;
    setUploading(true);
    const uploaded = await uploadAttachments(files, scope);
    setAttachments((current) => [
      ...current,
      ...uploaded.filter(
        (next) => !current.some((item) => item.id === next.id),
      ),
    ]);
    setUploading(false);
  };
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    try {
      void getCurrentWebview()
        .onDragDropEvent((event) => {
          const zone = inputZoneRef.current?.getBoundingClientRect();
          const position =
            "position" in event.payload ? event.payload.position : null;
          const scale = window.devicePixelRatio || 1;
          const inside = Boolean(
            zone &&
              position &&
              position.x / scale >= zone.left &&
              position.x / scale <= zone.right &&
              position.y / scale >= zone.top &&
              position.y / scale <= zone.bottom,
          );
          if (event.payload.type === "leave") setDragging(false);
          else if (event.payload.type === "over") setDragging(inside);
          else if (event.payload.type === "drop") {
            setDragging(false);
            if (inside) {
              setUploading(true);
              void uploadAttachmentPaths(event.payload.paths, scope).then(
                (uploaded) => {
                  setAttachments((current) => [...current, ...uploaded]);
                  setUploading(false);
                },
              );
            }
          }
        })
        .then((cleanup) => {
          if (disposed) cleanup();
          else unlisten = cleanup;
        })
        .catch(() => undefined);
    } catch {
      // 非 Tauri 环境不注册原生拖放。
    }
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [scope, uploadAttachmentPaths]);
  const submit = () => {
    if (!ready || uploading || (sending && !activeId)) return;
    const text = prompt.trim();
    const selection = selectedText.trim();
    const hasAttachments = attachments.length > 0;
    if (!text && !selection && !hasAttachments) return;

    const outgoingText = requestText(text, selection || undefined, includePage ? activePage : null);
    const outgoing = attachments;

    setPrompt("");
    setAttachments([]);
    onClearSelection?.();

    const dispatch = sending || queued.length > 0 ? enqueue : send;
    void dispatch({
      requestText: outgoingText,
      displayText: text,
      selectedText: selection ? selectedText : undefined,
      attachments: outgoing,
      scope,
    });
  };

  return (
    <aside
      className="agent-panel flex min-h-0 flex-col overflow-hidden rounded-[22px] border border-white bg-[var(--qy-sidebar)] shadow-[var(--qy-panel-shadow)]"
      aria-label="Agent 面板"
    >
      <div className="relative flex h-10 items-center justify-between border-b border-[var(--qy-border)] px-3">
        <div className="flex min-w-0 items-center gap-2 text-[14px] font-semibold">
          <Bot size={16} />
          <span className="shrink-0">Agent</span>
          {activePage && (
            <span className="truncate text-[12px] font-medium text-[var(--qy-faint)]">
              {activePage.title}
            </span>
          )}
        </div>
        <div className="flex items-center gap-1">
          <button
            className="icon-button"
            title="历史对话"
            aria-label="历史对话"
            aria-expanded={historyOpen}
            onClick={() => setHistoryOpen((value) => !value)}
          >
            <History size={15} />
          </button>
          <button
            className="icon-button"
            title="新建对话"
            aria-label="新建对话"
            disabled={sending || uploading}
            onClick={() => void create(scope)}
          >
            <MessageSquarePlus size={15} />
          </button>
          <button
            className="icon-button"
            title="关闭 Agent 面板"
            aria-label="关闭 Agent 面板"
            onClick={toggleAgentPanel}
          >
            <PanelRightClose size={16} />
          </button>
        </div>
        {historyOpen && (
          <div
            className="absolute right-2 top-9 z-20 max-h-80 w-[min(320px,calc(100vw-32px))] overflow-y-auto rounded-2xl border border-[var(--qy-border)] bg-white p-1.5 shadow-[var(--qy-float-shadow)]"
            aria-label="历史对话列表"
          >
            {sortedConversations.length ? (
              sortedConversations.map((conversation) => (
                <button
                  key={conversation.id}
                  type="button"
                  aria-label={`打开 ${conversation.title}`}
                  disabled={sending || uploading}
                  onClick={() => {
                    setHistoryOpen(false);
                    void open(conversation.id);
                  }}
                  className={`flex w-full items-center justify-between gap-3 rounded px-2 py-2 text-left hover:bg-[var(--qy-bg)] ${conversation.id === activeId ? "bg-[var(--qy-bg)]" : ""}`}
                >
                  <span className="min-w-0">
                    <span className="block truncate text-[13px] font-medium text-[var(--qy-ink)]">
                      {conversation.title}
                    </span>
                    <span className="mt-0.5 block text-[11px] text-[var(--qy-faint)]">
                      {scopeLabel(conversation.scope)} ·{" "}
                      {conversationTime(conversation.updatedAt)}
                    </span>
                  </span>
                  {conversation.id === activeId && (
                    <Check
                      className="shrink-0 text-[var(--qy-accent)]"
                      size={14}
                    />
                  )}
                </button>
              ))
            ) : (
              <div className="px-3 py-6 text-center text-[12px] text-[var(--qy-faint)]">
                暂无历史对话
              </div>
            )}
          </div>
        )}
      </div>
      <div ref={messageViewport} className="min-h-0 flex-1 overflow-y-auto px-3 py-3" onScroll={(event) => { const element = event.currentTarget; followReply.current = element.scrollHeight - element.clientHeight - element.scrollTop < 64; setShowLatest(!followReply.current); }}>
        {!ready && (
          <div className="flex h-full flex-col items-center justify-center text-center text-[13px] text-[var(--qy-muted)]">
            {status.state === "starting" ? (
              <LoaderCircle className="mb-3 animate-spin" size={22} />
            ) : (
              <Bot className="mb-3 text-[var(--qy-faint)]" size={24} />
            )}
            <p>
              {status.state === "failed"
                ? status.message
                : status.state === "starting"
                  ? "正在连接本机 Codex"
                  : "Codex 未连接"}
            </p>
            {status.state === "failed" && (
              <button
                className="mt-3 inline-flex items-center gap-1 text-[var(--qy-accent)]"
                onClick={() => void connect()}
              >
                <RotateCw size={13} />
                重新连接
              </button>
            )}
          </div>
        )}
        {ready && turns.length === 0 && (
          <div className="grid h-full place-items-center text-center text-[13px] font-medium text-[var(--qy-faint)]">
            <div>
              <Bot className="mx-auto mb-2" size={22} />
              <p>
                {activePage
                  ? `已识别当前页面：${activePage.title}`
                  : "已连接整个笔记库"}
              </p>
            </div>
          </div>
        )}
        {ready && <div className="qy-conversation-log" role="log" aria-label="对话消息">
          {firstVisible > 0 && <button type="button" className="qy-load-earlier" onClick={() => {
            const element = messageViewport.current; const height = element?.scrollHeight ?? 0;
            followReply.current = false; setVisibleCount((count) => count + 20);
            requestAnimationFrame(() => { if (element) element.scrollTop += element.scrollHeight - height; });
          }}>加载更早的对话</button>}
          {turns.slice(firstVisible).map((turn) => <ConversationTurn key={turn.key} turn={turn} expanded={expandedTurns[turn.key] ?? false} onToggle={() => setExpandedTurns((current) => ({ ...current, [turn.key]: !current[turn.key] }))}/>)}
        </div>}
        {error && !turns.some((turn) => turn.error === error) && <div className="mt-3 text-[12px] font-medium text-red-600" role="alert">{error}</div>}
      </div>
      <div className="qy-agent-composer-shell">
        {showLatest && <button type="button" className="qy-back-latest" onClick={() => { const element = messageViewport.current; if (element) element.scrollTop = element.scrollHeight; followReply.current = true; setShowLatest(false); }}>↓ 回到最新</button>}
        <div className="qy-agent-composer">
        <MessageQueue messages={queued} paused={paused} sending={sending} edit={editQueued} remove={removeQueued} resume={resumeQueue}/>
        <div className="qy-composer-context">{includePage && activePage && <span><button type="button" onClick={() => void openPage(activePage.id)}><FileText size={12}/>{activePage.title}</button><button type="button" aria-label="移除当前页面上下文" onClick={() => setIncludePage(false)}><X size={12}/></button></span>}{selectedText.trim() && <details><summary>引用 · {selectedText.match(/第\s*\d+(?:\s*[-–至]\s*\d+)?\s*行/)?.[0] ?? "选中文本"}</summary><pre>{selectedText}</pre></details>}</div>
        <input
          ref={attachmentInputRef}
          className="hidden"
          type="file"
          multiple
          aria-label="选择 Agent 附件"
          accept=".png,.jpg,.jpeg,.gif,.webp,.pdf,.docx,.pptx,.md,.txt,.json,.csv,.log,.xml"
          onChange={(event) => {
            const files = Array.from(event.currentTarget.files ?? []);
            event.currentTarget.value = "";
            void addFiles(files);
          }}
        />
        {selectedText.trim() && <button type="button" className="qy-clear-quote" aria-label="移除选中文本引用" onClick={() => onClearSelection?.()}>移除引用 ×</button>}
        {attachments.length > 0 && (
          <div className="mb-2 flex flex-wrap gap-1">
            {attachments.map((attachment) => (
              <AttachmentPreview
                key={attachment.id}
                attachment={attachment}
                removable
                onRemove={() =>
                  setAttachments((current) =>
                    current.filter((item) => item.id !== attachment.id),
                  )
                }
              />
            ))}
          </div>
        )}
        <div ref={inputZoneRef} aria-label="Agent 输入区" className={"agent-input qy-composer-card " + (dragging ? "is-dragging" : "")}
          onDragEnter={(event) => { event.preventDefault(); setDragging(true); }} onDragOver={(event) => event.preventDefault()}
          onDragLeave={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node)) setDragging(false); }}
          onDrop={(event) => { event.preventDefault(); setDragging(false); void addFiles(Array.from(event.dataTransfer.files)); }}>
          <textarea ref={promptRef} value={prompt} disabled={!ready}
            className="agent-prompt" placeholder={dragging ? "松开以添加文件" : "随心输入"}
            onFocus={() => { void invoke("window_refresh_ime_position").catch(() => undefined); }}
            onCompositionStart={() => { void invoke("window_refresh_ime_position").catch(() => undefined); }}
            onChange={(event) => setPrompt(event.target.value)}
            onPaste={(event) => { const images = clipboardImageFiles(event.clipboardData); if (images.length) { event.preventDefault(); void addFiles(images); } }}
            onKeyDown={(event) => { if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) { event.preventDefault(); submit(); } }}/>
          <div className="qy-composer-toolbar">
            <button type="button" className="icon-button" title="添加附件" aria-label="添加附件" disabled={uploading || !ready} onClick={() => attachmentInputRef.current?.click()}>{uploading ? <LoaderCircle size={16} className="animate-spin"/> : <span className="text-xl">+</span>}</button>
            <div ref={modelMenuRef} className="qy-composer-model">
              <button type="button" aria-label="选择模型" aria-haspopup="listbox" aria-expanded={modelMenuOpen} disabled={sending || !ready} title={selectedModelOption?.description || modelButtonLabel} onClick={() => setModelMenuOpen((value) => !value)}><Sparkles size={13}/><span>{modelButtonLabel}</span>{modelsLoading ? <LoaderCircle size={12} className="animate-spin"/> : <ChevronDown size={12}/>}</button>
              {modelMenuOpen && createPortal(<div ref={modelOptionsRef} role="listbox" aria-label="模型列表" className="qy-model-options" style={{ position: "fixed", ...modelPosition }}>
                <button type="button" role="option" aria-selected={selectedModel === null} onClick={() => { setSelectedModel(null); setModelMenuOpen(false); }}><span>跟随 Codex<small>{defaultModel ? "当前配置默认：" + defaultModel.displayName : "使用本机 Codex 默认模型"}</small></span>{selectedModel === null && <Check size={14}/>}</button>
                {models.map((item) => <button key={item.id} type="button" role="option" aria-selected={selectedModel === item.model} onClick={() => { setSelectedModel(item.model); setModelMenuOpen(false); }}><span>{item.displayName}{item.description && <small>{item.description}</small>}</span>{selectedModel === item.model && <Check size={14}/>}</button>)}
                {modelsError && <p>获取模型列表受限：{modelsError}</p>}
              </div>, document.body)}
            </div>
            {sending && <button type="button" className="qy-enqueue-button" aria-label="加入下一轮队列" disabled={uploading || !activeId || (!prompt.trim() && !selectedText.trim() && attachments.length === 0)} onClick={submit}>排队</button>}
            <button type="button" className={"agent-send-button icon-button " + (sending ? "is-sending" : "")} title={sending ? "停止" : "发送"} aria-label={sending ? "停止" : "发送消息"}
              disabled={!ready || (!sending && (uploading || (!prompt.trim() && !selectedText.trim() && attachments.length === 0)))}
              onClick={() => { if (sending) void interrupt(); else submit(); }}>{sending ? <Square size={13} className="fill-current"/> : <Send size={16}/>}</button>
          </div>
        </div>
        {modelsError && <div className="qy-model-warning"><span title={modelsError}>模型刷新受限</span><button type="button" aria-label="重试加载模型" onClick={() => void loadModels?.()}>重试</button></div>}
        </div>
      </div>
    </aside>
  );
});
