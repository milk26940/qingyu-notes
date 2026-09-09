import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type ConversationScope = { kind: "vault" } | { kind: "page"; pageId: string } | { kind: "database"; databaseId: string };
export type ConversationMetadata = { id: string; threadId: string | null; title: string; scope: ConversationScope; createdAt: string; updatedAt: string; activeTurnId: string | null };
export type ConversationRecord = { kind: string; timestamp: string; data: Record<string, unknown> };
export type ConversationAttachment = { id: string; name: string; mime: string; size: number; relativePath: string };
export type CodexItemSummary = { id?: string; itemType?: string; status?: string; tool?: string; namespace?: string };
export type CodexStreamEvent =
  | { kind: "agentMessageDelta"; threadId: string; turnId: string; itemId: string; delta: string }
  | { kind: "turnPlanUpdated"; threadId: string; turnId: string; explanation?: string; plan: unknown }
  | { kind: "turnDiffUpdated"; threadId: string; turnId: string; diff: string }
  | { kind: "turnStarted" | "turnCompleted"; threadId: string; turn: Record<string, unknown> }
  | { kind: "itemStarted" | "itemCompleted"; threadId: string; turnId: string; item: CodexItemSummary }
  | { kind: "knowledgeSearchResult"; threadId: string; turnId: string; callId: string; result: Record<string, unknown> }
  | { kind: "other"; method: string; params: unknown };
export type PendingApproval = { id: string; requestId: unknown; threadId: string; turnId: string; namespace: string; tool: string; arguments: unknown; diff: string };
export type ConversationPersistenceError = { code: string; message: string };

export const createConversation = (title: string, scope: ConversationScope, model?: string | null) =>
  invoke<ConversationMetadata>("conversation_create", { title, scope, model: model ?? undefined });
export const listConversations = () => invoke<ConversationMetadata[]>("conversation_list");
export const getConversationHistory = (id: string) => invoke<ConversationRecord[]>("conversation_history", { id });
export const sendConversationMessage = (
  id: string,
  text: string,
  attachments: ConversationAttachment[] = [],
  displayText?: string,
  selectedText?: string,
  model?: string | null,
) => invoke<{ threadId: string; turnId: string }>("conversation_send", {
  id,
  text,
  attachments,
  displayText,
  selectedText,
  model: model ?? undefined,
});
export const storeConversationAttachment = (id: string, originalName: string, mime: string, bytes: number[]) => invoke<ConversationAttachment>("conversation_store_attachment", { id, originalName, mime, bytes });
export const storeConversationAttachmentPath = (id: string, sourcePath: string) => invoke<ConversationAttachment>("conversation_store_attachment_path", { id, sourcePath });
export const readConversationAttachment = (relativePath: string, mime: string) => invoke<{ mime: string; bytes: number[] }>("conversation_read_attachment", { relativePath, mime });
export const interruptConversation = (threadId: string, turnId: string) => invoke<void>("conversation_interrupt", { threadId, turnId });
export const listApprovals = () => invoke<PendingApproval[]>("approval_list");
export const resolveApproval = (id: string, approved: boolean) => invoke<void>("approval_resolve", { id, approved });
export const subscribeConversationEvents = (handler: (event: CodexStreamEvent) => void): Promise<UnlistenFn> => listen<CodexStreamEvent>("codex://event", (event) => handler(event.payload));
export const subscribeApprovals = (handler: (approval: PendingApproval) => void): Promise<UnlistenFn> => listen<PendingApproval>("codex://approval", (event) => handler(event.payload));
export const subscribePersistenceErrors = (handler: (error: ConversationPersistenceError) => void): Promise<UnlistenFn> => listen<ConversationPersistenceError>("codex://persistence-error", (event) => handler(event.payload));
