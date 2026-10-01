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
import { useEffect, useMemo, useRef, useState } from "react";
import type { ConversationScope } from "../../lib/tauri/conversations";
import type { ConversationAttachment } from "../../lib/tauri/conversations";
import { readConversationAttachment } from "../../lib/tauri/conversations";
import { clipboardImageFiles } from "../editor/imageAttachments";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { useAgentStore } from "../../stores/agentStore";
import { useCodexStore } from "../../stores/codexStore";
import { usePagesStore } from "../../stores/pagesStore";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { invoke } from "@tauri-apps/api/core";
import DOMPurify from "dompurify";
import { marked } from "marked";
import { parseQingyuPageLink } from "../../lib/internalLinks";

function AgentMarkdown({ text }: { text: string }) {
  const openPage = usePagesStore((state) => state.open);
  const html = useMemo(
    () => DOMPurify.sanitize(marked.parse(text, { gfm: true, breaks: true }) as string, {
      ALLOWED_URI_REGEXP: /^(?:(?:https?|mailto|qingyu):|[^a-z]|[a-z+.-]+(?:[^a-z+.-:]|$))/i,
    }),
    [text],
  );
  return <div className="qy-agent-markdown" onClick={(event) => {
    const anchor = (event.target as HTMLElement).closest("a");
    if (!anchor) return;
    const target = parseQingyuPageLink(anchor.getAttribute("href") ?? "");
    if (!target) return;
    event.preventDefault();
    void openPage(target.pageId, target.blockId);
  }} dangerouslySetInnerHTML={{ __html: html }} />;
}

function buildRequestText({
  prompt,
  selectedText,
  activePage,
}: {
  prompt: string;
  selectedText?: string;
  activePage?: { id: string; title: string } | null;
}) {
  const parts: string[] = [];
  const text = prompt.trim();
  if (text) {
    parts.push(text);
  }
  const selection = selectedText?.trim();
  if (selection) {
    parts.push(`选中文本：\n> ${selection.replace(/\n/g, "\n> ")}`);
  }
  if (activePage) {
    parts.push(
      `当前页面：${activePage.title}\n页面 ID：${activePage.id}\n需要正文时调用 page.read/page.read_document 按需读取`,
    );
  }
  return parts.join("\n\n");
}

function visibleMessages(
  records: ReturnType<typeof useAgentStore.getState>["records"],
) {
  const messages: {
    role: "user" | "assistant";
    text: string;
    itemId?: string;
    turnId?: string;
    selectedText?: string;
    attachments?: ConversationAttachment[];
  }[] = [];
  for (const item of records) {
    if (item.kind === "userMessage") {
      let text = "";
      let selectedText: string | undefined;
      if (item.data.displayText !== undefined) {
        text = String(item.data.displayText ?? "");
        selectedText = item.data.selectedText ? String(item.data.selectedText) : undefined;
      } else {
        const raw = String(item.data.text ?? "");
        const pageIdx = raw.indexOf("\n\n当前页面：");
        const withoutPage = pageIdx >= 0 ? raw.slice(0, pageIdx) : raw;
        const selIdx = withoutPage.indexOf("\n\n选中文本：\n");
        if (selIdx >= 0) {
          text = withoutPage.slice(0, selIdx).trim();
          const selRaw = withoutPage.slice(selIdx + "\n\n选中文本：\n".length).trim();
          selectedText = selRaw.replace(/^>\s?/gm, "");
        } else {
          text = withoutPage.trim();
        }
      }
      messages.push({
        role: "user",
        text,
        selectedText,
        attachments:
          (item.data.attachments as ConversationAttachment[] | undefined) ?? [],
      });
    }
    if (item.kind === "agentMessageDelta") {
      const last = messages[messages.length - 1];
      const itemId = item.data.itemId ? String(item.data.itemId) : undefined;
      const turnId = item.data.turnId ? String(item.data.turnId) : undefined;
      if (last?.role === "assistant" && last.itemId === itemId)
        last.text += String(item.data.delta ?? "");
      else
        messages.push({
          role: "assistant",
          itemId,
          turnId,
          text: String(item.data.delta ?? ""),
        });
    }
    if (item.kind === "turnCompleted" && ["failed", "error"].includes(String(item.data.status ?? "completed"))) {
      const failure = item.data.error;
      const text = failure && typeof failure === "object" && "message" in failure
        ? String((failure as { message?: unknown }).message ?? "Agent 请求失败")
        : String(failure ?? "Agent 请求失败");
      messages.push({ role: "assistant", text: `请求失败：${text}` });
    }
    if (item.kind === "turnCompleted" && String(item.data.status ?? "") === "interrupted") {
      messages.push({ role: "assistant", text: "本轮已停止" });
    }
  }
  return messages;
}

function AssistantReply({ messages, index, sending }: {
  messages: ReturnType<typeof visibleMessages>; index: number; sending: boolean;
}) {
  const nextUser = messages.findIndex((item, position) => position > index && item.role === "user");
  const end = nextUser < 0 ? messages.length : nextUser;
  const group = messages.slice(index, end);
  const last = group[group.length - 1];
  const previous = group.slice(0, -1);
  const process = previous.filter((item) => item.itemId);
  const legacy = previous.filter((item) => !item.itemId);
  return <div>
    {process.length > 0 && <details className="mb-3 rounded-xl bg-[#f3f4f6] px-3 py-2">
      <summary className="cursor-pointer text-sm font-semibold text-[#454b55]">处理过程 · {process.length} 条</summary>
      <div className="mt-2 space-y-3">{process.map((item, position) => <div key={item.itemId ?? position} className="border-t border-[#d5d8dd] pt-2"><AgentMarkdown text={item.text} /></div>)}</div>
    </details>}
    {legacy.map((item, position) => <AgentMarkdown key={position} text={item.text} />)}
    <div className="border-t border-[#d5d8dd] pt-3">
      <div className="mb-2 text-xs font-semibold text-[#515965]">{sending && nextUser < 0 ? "正在回复" : "回复"}</div>
      <AgentMarkdown text={last.text} />
    </div>
  </div>;
}

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

function progressStatus(status?: string) {
  if (status === "completed") return "已完成";
  if (status === "failed" || status === "error") return "失败";
  if (status === "cancelled" || status === "canceled") return "已取消";
  return "进行中";
}

function AttachmentPreview({
  attachment,
  removable,
  onRemove,
}: {
  attachment: ConversationAttachment;
  removable?: boolean;
  onRemove?: () => void;
}) {
  const [source, setSource] = useState("");
  useEffect(() => {
    if (!attachment.mime.startsWith("image/")) return;
    let active = true;
    let url = "";
    void readConversationAttachment(attachment.relativePath, attachment.mime)
      .then((data) => {
        if (!active) return;
        url = URL.createObjectURL(
          new Blob([new Uint8Array(data.bytes)], { type: data.mime }),
        );
        setSource(url);
      })
      .catch(() => undefined);
    return () => {
      active = false;
      if (url) URL.revokeObjectURL(url);
    };
  }, [attachment.mime, attachment.relativePath]);
  return (
    <div className="relative flex max-w-full items-center gap-1 rounded border border-[var(--qy-border)] bg-white p-1 text-[12px]">
      {source ? (
        <img
          src={source}
          alt={attachment.name}
          className="h-14 w-14 rounded object-cover"
        />
      ) : (
        <FileText size={14} />
      )}
      <span className="max-w-36 truncate px-1">{attachment.name}</span>
      {removable && (
        <button
          type="button"
          className="self-start"
          aria-label={`移除附件 ${attachment.name}`}
          onClick={onRemove}
        >
          <X size={12} />
        </button>
      )}
    </div>
  );
}

export function AgentPanel({
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
  const activePage = usePagesStore((state) => state.activePage);
  const reloadPage = usePagesStore((state) => state.reload);
  const storeSelectedModel = useAgentStore((state) => state?.selectedModel);
  const selectedModel = typeof storeSelectedModel === "string" ? storeSelectedModel : null;
  const setSelectedModel = useAgentStore((state) => state?.setSelectedModel) ?? (() => undefined);
  const {
    conversations,
    activeId,
    records,
    plan,
    diff,
    tools,
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
    if (previousSending.current && !sending && activePage) {
      void reloadPage(activePage.id);
    }
    previousSending.current = sending;
  }, [activePage, reloadPage, sending]);
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
  const messages = visibleMessages(records);
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
    const handleClickOutside = (event: MouseEvent) => {
      if (modelMenuRef.current && !modelMenuRef.current.contains(event.target as Node)) {
        setModelMenuOpen(false);
      }
    };
    window.addEventListener("mousedown", handleClickOutside);
    return () => window.removeEventListener("mousedown", handleClickOutside);
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
    if (!ready || sending || uploading) return;
    const text = prompt.trim();
    const selection = selectedText.trim();
    const hasAttachments = attachments.length > 0;
    if (!text && !selection && !hasAttachments) return;

    const requestText = buildRequestText({
      prompt: text,
      selectedText: selection ? selectedText : undefined,
      activePage,
    });
    const outgoing = attachments;

    setPrompt("");
    setAttachments([]);
    onClearSelection?.();

    void send({
      requestText,
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
      <div className="min-h-0 flex-1 overflow-y-auto px-3 py-3">
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
        {ready && messages.length === 0 && (
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
        {ready && (
          <div className="space-y-3">
            {messages.map((message, index) => message.role === "assistant" && messages[index - 1]?.role === "assistant" ? null : (
              <div
                key={`${message.role}-${index}`}
                className={
                  message.role === "user"
                    ? "ml-8 rounded-[16px] border border-[#d7dbe0] bg-white px-3.5 py-2.5 text-[13px] font-medium leading-6 text-[var(--qy-ink)] shadow-sm"
                    : "px-1 text-[14px] font-normal leading-relaxed text-[var(--qy-ink)]"
                }
              >
                {message.role === "assistant" ? (
                  <AssistantReply messages={messages} index={index} sending={sending} />
                ) : (
                  <>
                    {message.selectedText && (
                      <div className="mb-2 rounded-xl border border-[#d7dbe0] bg-[#f8f9fb] p-2 text-[12px] text-[var(--qy-muted)]">
                        <div className="flex items-center gap-1 font-semibold text-[11px] text-[var(--qy-faint)]">
                          <FileText size={12} />
                          <span>选中文本引用</span>
                        </div>
                        <div className="mt-1 max-h-24 overflow-y-auto whitespace-pre-wrap font-mono text-[11px] leading-5 text-[var(--qy-ink)]">
                          {message.selectedText}
                        </div>
                      </div>
                    )}
                    {message.text && <div className="whitespace-pre-wrap">{message.text}</div>}
                    {message.attachments && message.attachments.length > 0 && (
                      <div className="mt-2 flex flex-wrap gap-1">
                        {message.attachments.map((attachment) => (
                          <AttachmentPreview
                            key={attachment.id}
                            attachment={attachment}
                          />
                        ))}
                      </div>
                    )}
                  </>
                )}
              </div>
            ))}
            {sending && (
              <div className="flex items-center gap-2 text-[12px] font-medium text-[var(--qy-faint)]">
                <LoaderCircle className="animate-spin" size={13} />
                Agent 正在工作
              </div>
            )}
          </div>
        )}
        {tools.length > 0 && (
          <details className="mt-3 border-t border-[var(--qy-border)] pt-2">
            <summary className="cursor-pointer text-[12px] font-medium text-[var(--qy-muted)]">
              工具进度 · {tools.length}
            </summary>
            {tools.map((tool) => (
              <div
                key={tool.id}
                className="py-1 text-[12px] text-[var(--qy-faint)]"
              >
                {tool.namespace ? `${tool.namespace}.` : ""}
                {tool.tool ?? tool.itemType} · {progressStatus(tool.status)}
              </div>
            ))}
          </details>
        )}
        {plan != null && (
          <details className="mt-3 border-t border-[var(--qy-border)] pt-2">
            <summary className="cursor-pointer text-[12px] font-medium text-[var(--qy-muted)]">
              执行计划
            </summary>
            <pre className="mt-2 whitespace-pre-wrap text-[11px] leading-5 text-[var(--qy-faint)]">
              {JSON.stringify(plan, null, 2)}
            </pre>
          </details>
        )}
        {diff && (
          <details className="mt-3 border-t border-[var(--qy-border)] pt-2">
            <summary className="cursor-pointer text-[12px] font-medium text-[var(--qy-muted)]">
              变更 diff
            </summary>
            <pre className="mt-2 max-h-48 overflow-auto whitespace-pre-wrap bg-white p-2 text-[11px] leading-5">
              {diff}
            </pre>
          </details>
        )}
        {approvals.map((approval) => (
          <div
            key={approval.id}
            className="mt-3 border-t border-[var(--qy-border)] pt-2"
          >
            <div className="text-[13px] font-semibold">
              等待审批：{approval.namespace}.{approval.tool}
            </div>
            <pre className="my-2 max-h-32 overflow-auto whitespace-pre-wrap bg-white p-2 text-[11px] leading-5">
              {approval.diff}
            </pre>
            <div className="flex justify-end gap-1">
              <button
                className="inline-flex h-7 items-center gap-1 rounded-md px-2 text-[12px] text-red-600 hover:bg-white"
                onClick={() => void resolveApproval(approval.id, false)}
              >
                <X size={13} />
                拒绝
              </button>
              <button
                className="inline-flex h-7 items-center gap-1 rounded-md bg-[var(--qy-accent)] px-2 text-[12px] text-white"
                onClick={() => void resolveApproval(approval.id, true)}
              >
                <Check size={13} />
                批准
              </button>
            </div>
          </div>
        ))}
        {error && <div className="mt-3 text-[12px] font-medium text-red-600">{error}</div>}
      </div>
      <div className="border-t border-[var(--qy-border)] p-3">
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
        {selectedText.trim() && (
          <div className="mb-2 rounded-xl border border-[var(--qy-border)] bg-white p-2 text-[12px] shadow-sm">
            <div className="flex items-center justify-between">
              <span className="font-medium text-[var(--qy-ink)]">1 条注释</span>
              <button
                type="button"
                className="text-[var(--qy-faint)] hover:text-[var(--qy-ink)]"
                aria-label="移除选中文本引用"
                onClick={() => onClearSelection?.()}
              >
                ×
              </button>
            </div>
            <div className="mt-1 max-h-12 overflow-hidden whitespace-pre-wrap text-[var(--qy-muted)]">
              {selectedText}
            </div>
          </div>
        )}
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
        <div className="mb-2 flex items-center justify-between gap-2 px-0.5">
          <div ref={modelMenuRef} className="relative">
            <button
              type="button"
              aria-label="选择模型"
              aria-haspopup="listbox"
              aria-expanded={modelMenuOpen}
              disabled={sending || !ready}
              onClick={() => setModelMenuOpen((open) => !open)}
              title={
                selectedModelOption?.description ||
                (defaultModel && !selectedModel
                  ? `跟随本机 Codex（当前默认：${defaultModel.displayName}）`
                  : "选择对话模型")
              }
              className="inline-flex h-6 max-w-[240px] items-center gap-1.5 rounded-full border border-[var(--qy-border)] bg-white px-2.5 text-[11px] font-medium text-[var(--qy-muted)] shadow-[0_1px_2px_rgba(0,0,0,0.03)] transition-colors hover:bg-[var(--qy-bg)] hover:text-[var(--qy-ink)] disabled:cursor-not-allowed disabled:opacity-50"
            >
              <Sparkles size={11} className="shrink-0 text-[var(--qy-accent)]" />
              <span className="truncate">{modelButtonLabel}</span>
              {modelsLoading ? (
                <LoaderCircle size={10} className="shrink-0 animate-spin text-[var(--qy-faint)]" />
              ) : (
                <ChevronDown size={10} className="shrink-0 text-[var(--qy-faint)]" />
              )}
            </button>

            {modelMenuOpen && (
              <div
                role="listbox"
                aria-label="模型列表"
                className="absolute bottom-full left-0 z-30 mb-1.5 max-h-64 w-72 overflow-y-auto rounded-2xl border border-[var(--qy-border)] bg-white p-1.5 shadow-[var(--qy-float-shadow)]"
              >
                <button
                  type="button"
                  role="option"
                  aria-selected={selectedModel === null}
                  onClick={() => {
                    setSelectedModel(null);
                    setModelMenuOpen(false);
                  }}
                  className={`flex w-full items-center justify-between gap-2 rounded-xl px-2.5 py-1.5 text-left transition-colors hover:bg-[var(--qy-bg)] ${selectedModel === null ? "bg-[var(--qy-bg)]" : ""}`}
                >
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-1 text-[12px] font-medium text-[var(--qy-ink)]">
                      <span>跟随 Codex</span>
                      <span className="rounded bg-[#e9ebef] px-1 py-0.5 text-[9px] font-normal text-[var(--qy-faint)]">
                        推荐
                      </span>
                    </div>
                    <div className="truncate text-[10px] text-[var(--qy-faint)]">
                      {defaultModel ? `当前配置默认：${defaultModel.displayName}` : "使用本机 Codex 默认模型"}
                    </div>
                  </div>
                  {selectedModel === null && (
                    <Check size={13} className="shrink-0 text-[var(--qy-accent)]" />
                  )}
                </button>

                {models.length > 0 && <div className="my-1 border-t border-[var(--qy-border)]" />}

                {models.map((item) => (
                  <button
                    key={item.id}
                    type="button"
                    role="option"
                    aria-selected={selectedModel === item.model}
                    onClick={() => {
                      setSelectedModel(item.model);
                      setModelMenuOpen(false);
                    }}
                    className={`flex w-full items-center justify-between gap-2 rounded-xl px-2.5 py-1.5 text-left transition-colors hover:bg-[var(--qy-bg)] ${selectedModel === item.model ? "bg-[var(--qy-bg)]" : ""}`}
                  >
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-1.5 text-[12px] font-medium text-[var(--qy-ink)]">
                        <span className="truncate">{item.displayName}</span>
                        {item.isDefault && (
                          <span className="rounded bg-[#e9ebef] px-1 py-0.5 text-[9px] font-normal text-[var(--qy-faint)]">
                            默认
                          </span>
                        )}
                      </div>
                      {item.description && (
                        <div className="truncate text-[10px] text-[var(--qy-faint)]" title={item.description}>
                          {item.description}
                        </div>
                      )}
                    </div>
                    {selectedModel === item.model && (
                      <Check size={13} className="shrink-0 text-[var(--qy-accent)]" />
                    )}
                  </button>
                ))}

                {modelsLoading && (
                  <div className="px-2 py-2 text-center text-[11px] text-[var(--qy-faint)]">
                    <LoaderCircle size={12} className="mr-1 inline animate-spin" />
                    正在刷新可用模型...
                  </div>
                )}

                {modelsError && (
                  <div className="mt-1 border-t border-[var(--qy-border)] px-2 py-1.5 text-[10px] text-amber-700">
                    <div>获取模型列表受限：{modelsError}</div>
                    {loadModels && (
                      <button
                        type="button"
                        onClick={() => void loadModels()}
                        className="mt-0.5 text-[var(--qy-accent)] hover:underline"
                      >
                        重新加载
                      </button>
                    )}
                  </div>
                )}
              </div>
            )}
          </div>

          {modelsError && !modelMenuOpen && (
            <div className="flex items-center gap-1 text-[10px] text-amber-600">
              <span className="max-w-[120px] truncate" title={modelsError}>
                模型刷新受限
              </span>
              {loadModels && (
                <button
                  type="button"
                  aria-label="重试加载模型"
                  onClick={() => void loadModels()}
                  className="text-[var(--qy-accent)] hover:underline"
                >
                  重试
                </button>
              )}
            </div>
          )}
        </div>
        <div
          ref={inputZoneRef}
          aria-label="Agent 输入区"
          className={`agent-input flex items-end gap-2 rounded-[18px] border bg-white p-3 ${dragging ? "border-[var(--qy-accent)] bg-blue-50" : "border-[var(--qy-border)]"}`}
          onDragEnter={(event) => {
            event.preventDefault();
            setDragging(true);
          }}
          onDragOver={(event) => event.preventDefault()}
          onDragLeave={(event) => {
            if (!event.currentTarget.contains(event.relatedTarget as Node))
              setDragging(false);
          }}
          onDrop={(event) => {
            event.preventDefault();
            setDragging(false);
            void addFiles(Array.from(event.dataTransfer.files));
          }}
        >
          <button
            type="button"
            className="icon-button shrink-0"
            title="添加附件"
            aria-label="添加附件"
            disabled={uploading}
            onClick={() => attachmentInputRef.current?.click()}
          >
            {uploading ? (
              <LoaderCircle className="animate-spin" size={15} />
            ) : (
              <span className="text-lg leading-none">+</span>
            )}
          </button>
          <textarea
            ref={promptRef}
            value={prompt}
            onFocus={() => {
              void invoke("window_refresh_ime_position").catch(() => undefined);
            }}
            onCompositionStart={() => {
              void invoke("window_refresh_ime_position").catch(() => undefined);
            }}
            onChange={(event) => setPrompt(event.target.value)}
            onPaste={(event) => {
              const images = clipboardImageFiles(event.clipboardData);
              if (images.length) {
                event.preventDefault();
                void addFiles(images);
              }
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter" && !event.shiftKey) {
                event.preventDefault();
                if (!sending) {
                  submit();
                }
              }
            }}
            disabled={!ready}
            className="agent-prompt min-h-16 min-w-0 flex-1 resize-none overflow-y-auto px-1 py-2 text-[13px] font-medium leading-6 outline-none"
            placeholder={dragging ? "松开以添加文件" : "随心输入"}
          />
          <button
            type="button"
            className={`agent-send-button icon-button h-9 w-9 shrink-0 rounded-full text-white shadow-md transition-colors ${
              sending
                ? "is-sending bg-[#191b1f] hover:bg-[#32363d]"
                : "bg-[#3478f6] hover:bg-[#2468e8]"
            }`}
            title={sending ? "停止" : "发送"}
            aria-label={sending ? "停止" : "发送消息"}
            disabled={
              sending
                ? false
                : (!prompt.trim() && !selectedText.trim() && attachments.length === 0) ||
                  !ready ||
                  uploading
            }
            onClick={() => {
              if (sending) {
                void interrupt();
              } else {
                submit();
              }
            }}
          >
            {sending ? <Square size={13} className="fill-current" /> : <Send size={15} />}
          </button>
        </div>
      </div>
    </aside>
  );
}
