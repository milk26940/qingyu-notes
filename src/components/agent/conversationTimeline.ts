import type { CodexItemSummary, ConversationAttachment, ConversationRecord } from "../../lib/tauri/conversations";

export type ContextPage = { id: string; title: string };
export type TimelineMessage = { id: string; text: string; phase?: string | null };
export type TimelineTool = { id: string; namespace?: string; tool: string; status: string; arguments?: unknown; error?: string };
export type ChangeSnapshot = { pageId: string; title: string; content: string };
export type TimelineChange = { id: string; tool: string; before?: ChangeSnapshot; after?: ChangeSnapshot; blockId?: string; preview?: string; confirmed: boolean };
export type TimelineTurn = {
  key: string; turnId?: string; startedAt: number; endedAt?: number;
  user?: { text: string; selectedText?: string; attachments: ConversationAttachment[]; page?: ContextPage };
  messages: TimelineMessage[]; tools: TimelineTool[]; changes: TimelineChange[];
  plan?: unknown; status: string; error?: string;
};
const time = (value: string) => { const number = Number(value); return number < 1e12 ? number * 1000 : number; };
const object = (value: unknown): Record<string, any> => value && typeof value === "object" ? value as Record<string, any> : {};
export function requestText(prompt: string, selectedText?: string, page?: ContextPage | null) {
  return [prompt.trim(), selectedText?.trim() ? "选中文本：\n> " + selectedText.trim().replace(/\n/g, "\n> ") : "", page ? "当前页面：" + page.title + "\n页面 ID：" + page.id + "\n需要正文时调用 page.read/page.read_document 按需读取" : ""].filter(Boolean).join("\n\n");
}
export function userContext(data: Record<string, any>) {
  const raw = String(data.text ?? "");
  const pageMatch = raw.match(/(?:^|\n\n)当前页面：([^\n]*)\n页面 ID：([^\n]+)/);
  const withoutPage = raw.split("\n\n当前页面：")[0];
  const pieces = withoutPage.split(/(?:^|\n\n)选中文本：\n/);
  return {
    text: data.displayText !== undefined ? String(data.displayText) : pieces[0].trim(),
    selectedText: data.selectedText ? String(data.selectedText) : pieces.length > 1 ? pieces[1].replace(/^>\s?/gm, "").trim() : undefined,
    page: pageMatch ? { title: pageMatch[1], id: pageMatch[2].trim() } : undefined,
    attachments: (data.attachments ?? []) as ConversationAttachment[],
  };
}

// 以轮次 ID 归属事件，缺失 ID 的旧记录只按原有顺序归属，不推测最终答复。
export function conversationTimeline(records: ConversationRecord[], activeTurnId: string | null, sending: boolean): TimelineTurn[] {
  const turns: TimelineTurn[] = [];
  const byId = new Map<string, TimelineTurn>();
  const detailedTools = new Set<string>();
  let current: TimelineTurn | undefined;
  const newTurn = (record: ConversationRecord, index: number) => {
    const turn: TimelineTurn = { key: "turn-" + index, startedAt: time(record.timestamp), messages: [], tools: [], changes: [], status: "unknown" };
    turns.push(turn); return turn;
  };
  records.forEach((record, index) => {
    const data = object(record.data);
    if (record.kind === "userMessage") { current = newTurn(record, index); current.user = userContext(data); return; }
    const id = String(data.turnId ?? (["turnStarted", "turnCompleted"].includes(record.kind) ? data.id ?? "" : ""));
    let turn = id ? byId.get(id) : current;
    if (!turn) {
      turn = current && !current.turnId ? current : newTurn(record, index);
      if (id) { turn.turnId = id; byId.set(id, turn); }
    }
    if (id && !turn.turnId) { turn.turnId = id; byId.set(id, turn); }
    current = turn;
    const item = object(data.item) as CodexItemSummary;
    if (record.kind === "agentMessageDelta" || ((record.kind === "itemStarted" || record.kind === "itemCompleted") && item.itemType === "agentMessage")) {
      const messageId = String(data.itemId ?? item.id ?? "legacy-" + index);
      let message = turn.messages.find((entry) => entry.id === messageId);
      if (!message) { message = { id: messageId, text: "" }; turn.messages.push(message); }
      if (record.kind === "agentMessageDelta") message.text += String(data.delta ?? "");
      else { if (item.phase) message.phase = item.phase; if (item.text) message.text = item.text; }
    }
    if (record.kind === "turnStarted") { turn.status = "inProgress"; turn.startedAt = time(record.timestamp); }
    if (record.kind === "turnCompleted") {
      turn.status = String(data.status ?? "completed"); turn.endedAt = time(record.timestamp);
      if (data.error) turn.error = String(object(data.error).message ?? data.error);
    }
    if (record.kind === "planUpdated") turn.plan = data.plan;
    if (["itemStarted", "itemCompleted"].includes(record.kind) && item.itemType !== "agentMessage" && item.itemType !== "reasoning" && item.itemType !== "userMessage") {
      const toolId = item.id ?? "item-" + index;
      const existing = turn.tools.find((tool) => tool.id === toolId);
      const entry = { id: toolId, namespace: item.namespace, tool: item.tool ?? item.itemType ?? "工具操作", status: item.status ?? (record.kind === "itemCompleted" ? "completed" : "inProgress") };
      if (existing) { if (!detailedTools.has(toolId)) Object.assign(existing, entry); } else turn.tools.push(entry);
    }
    if (["toolActivity", "toolRequest", "toolCompleted"].includes(record.kind)) {
      const toolId = String(data.callId ?? "tool-" + index);
      detailedTools.add(toolId);
      let entry = turn.tools.find((tool) => tool.id === toolId);
      if (!entry) { entry = { id: toolId, tool: String(data.tool ?? "工具操作"), namespace: data.namespace, status: "inProgress" }; turn.tools.push(entry); }
      if (data.arguments) entry.arguments = data.arguments;
      if (record.kind === "toolCompleted" || data.stage === "completed") { entry.status = data.success ? "completed" : "failed"; entry.error = data.error ?? entry.error; }
      if (data.change) {
        const change = object(data.change);
        const value: TimelineChange = { id: toolId, tool: entry.tool, before: change.before ?? undefined, after: change.after ?? undefined, blockId: change.blockId, confirmed: data.success === true };
        const old = turn.changes.findIndex((candidate) => candidate.id === toolId);
        if (old < 0) turn.changes.push(value); else turn.changes[old] = value;
      }
    }
    if (record.kind === "changeSummary" && !turn.changes.some((change) => change.id === String(data.callId))) {
      turn.changes.push({ id: String(data.callId), tool: String(data.tool ?? "修改"), preview: String(data.diff ?? ""), confirmed: turn.tools.some((tool) => tool.id === data.callId && tool.status === "completed") });
    }
    if (record.kind === "diffUpdated") {
      const old = turn.changes.find((change) => change.id === "preview");
      if (old) old.preview = String(data.diff ?? "");
      else turn.changes.push({ id: "preview", tool: "变更预览", preview: String(data.diff ?? ""), confirmed: false });
    }
  });
  for (const turn of turns) {
    const isActive = sending && (activeTurnId ? turn.turnId === activeTurnId : turn === turns[turns.length - 1]);
    if (isActive && !turn.endedAt) turn.status = "inProgress";
    else if (turn.status === "inProgress") turn.status = "unknown";
    if (!isActive) for (const tool of turn.tools) if (tool.status === "inProgress" || tool.status === "running") tool.status = "unknown";
    if (turn.changes.some((change) => change.id !== "preview")) turn.changes = turn.changes.filter((change) => change.id !== "preview");
  }
  return turns;
}

const toolNames: Record<string, string> = { read: "读取页面", read_document: "读取页面区块", search: "检索笔记", create: "新建页面", update: "更新页面", create_block: "新增区块", update_block: "修改区块", move_block: "移动区块", delete_block: "删除区块", archive: "归档页面", move: "移动页面", query: "查询数据库", create_record: "新建记录", update_record: "更新记录", update_schema: "修改数据库字段", backlinks: "读取反向链接", link: "建立关联", unlink: "移除关联" };
export const toolLabel = (tool: TimelineTool) => tool.namespace === "attachment" && tool.tool === "read" ? "读取附件" : toolNames[tool.tool] ?? ({ commandExecution: "执行命令", dynamicToolCall: "调用笔记工具", fileChange: "修改文件" }[tool.tool] ?? tool.tool);
export const statusLabel = (status: string) => ({ completed: "已完成", inProgress: "进行中", running: "进行中", failed: "失败", error: "失败", interrupted: "已停止", cancelled: "已取消", canceled: "已取消", unknown: "结果待确认" }[status] ?? "结果待确认");
