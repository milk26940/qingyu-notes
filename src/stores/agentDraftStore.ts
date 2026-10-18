import { create } from "zustand";
import type { ContextReference, ConversationAttachment } from "../lib/tauri/conversations";

export type AgentDraft = {
  prompt: string;
  attachments: ConversationAttachment[];
  references: ContextReference[];
};

type AgentDraftStore = {
  drafts: Record<string, AgentDraft>;
  targetConversationId: string | null;
  setTargetConversation: (conversationId: string | null) => void;
  setPrompt: (key: string, prompt: string) => void;
  setAttachments: (key: string, attachments: ConversationAttachment[]) => void;
  addReferences: (references: ContextReference[], conversationId?: string | null) => void;
  removeReference: (key: string, id: string) => void;
  clearReferences: (key?: string) => void;
  clearDraft: (key: string) => void;
  migrateDraft: (from: string, to: string) => void;
  consumeDraft: (key: string, submitted: AgentDraft) => void;
};

export const EMPTY_AGENT_DRAFT: AgentDraft = { prompt: "", attachments: [], references: [] };
export const agentDraftKey = (conversationId: string | null | undefined) => conversationId || "draft";

function referenceScopeKey(reference: ContextReference): string {
  if (reference.source === "page") {
    return ["page", reference.pageId, reference.pageRevision, reference.blockIds?.join(","), reference.from, reference.to, reference.text].join(":");
  }
  return ["conversation", reference.conversationId, reference.messageId, reference.from, reference.to, reference.text].join(":");
}

function updateDraft(
  drafts: Record<string, AgentDraft>,
  key: string,
  update: (draft: AgentDraft) => AgentDraft,
): Record<string, AgentDraft> {
  return { ...drafts, [key]: update(drafts[key] ?? EMPTY_AGENT_DRAFT) };
}

export const useAgentDraftStore = create<AgentDraftStore>((set, get) => ({
  drafts: {},
  targetConversationId: null,
  migrateDraft: (from, to) => set((state) => {
    if (from === to || !state.drafts[from]) return {};
    const drafts = { ...state.drafts, [to]: state.drafts[from] };
    delete drafts[from];
    return { drafts, targetConversationId: to === "draft" ? null : to };
  }),
  consumeDraft: (key, submitted) => set((state) => ({ drafts: updateDraft(state.drafts, key, (draft) => ({
    prompt: draft.prompt === submitted.prompt ? "" : draft.prompt,
    attachments: draft.attachments.filter((item) => !submitted.attachments.some((sent) => sent.id === item.id)),
    references: draft.references.filter((item) => !submitted.references.some((sent) => sent.id === item.id)),
  })) })),
  setTargetConversation: (targetConversationId) => set({ targetConversationId }),
  setPrompt: (key, prompt) => set((state) => ({ drafts: updateDraft(state.drafts, key, (draft) => ({ ...draft, prompt })) })),
  setAttachments: (key, attachments) => set((state) => ({ drafts: updateDraft(state.drafts, key, (draft) => ({ ...draft, attachments })) })),
  addReferences: (references, conversationId) => {
    if (!references.length) return;
    const key = agentDraftKey(conversationId === undefined ? get().targetConversationId : conversationId);
    set((state) => ({
      drafts: updateDraft(state.drafts, key, (draft) => {
        const scopes = new Set(draft.references.map(referenceScopeKey));
        const additions = references.filter((reference) => {
          const scope = referenceScopeKey(reference);
          if (scopes.has(scope)) return false;
          scopes.add(scope);
          return true;
        });
        return additions.length ? { ...draft, references: [...draft.references, ...additions] } : draft;
      }),
    }));
  },
  removeReference: (key, id) => set((state) => ({ drafts: updateDraft(state.drafts, key, (draft) => ({ ...draft, references: draft.references.filter((reference) => reference.id !== id) })) })),
  clearReferences: (key) => set((state) => {
    if (key) return { drafts: updateDraft(state.drafts, key, (draft) => ({ ...draft, references: [] })) };
    return { drafts: Object.fromEntries(Object.entries(state.drafts).map(([draftKey, draft]) => [draftKey, { ...draft, references: [] }])) };
  }),
  clearDraft: (key) => set((state) => {
    const drafts = { ...state.drafts };
    delete drafts[key];
    return { drafts };
  }),
}));
