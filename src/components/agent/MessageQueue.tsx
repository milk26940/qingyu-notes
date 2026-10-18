import { Pencil, Play, X } from "lucide-react";
import { useState } from "react";
import type { QueuedMessage, SendMessageOptions } from "../../stores/agentStore";
import { requestText, userContext } from "./conversationTimeline";

export function MessageQueue({ messages, paused, sending, edit, remove, resume }: { messages: QueuedMessage[]; paused: boolean; sending: boolean; edit: (id: string, input: SendMessageOptions) => void; remove: (id: string) => void; resume: () => void }) {
  const [editing, setEditing] = useState<string | null>(null);
  const [text, setText] = useState("");
  if (!messages.length) return null;
  return <section className="qy-message-queue" aria-label="待发送消息队列"><header><span>排队下一轮 · {messages.length}{paused ? " · 已暂停" : ""}</span>{!sending && <button type="button" onClick={resume}><Play size={12}/>继续队列</button>}</header>
    <ol>{messages.map((message, index) => { const context = userContext({ text: message.input.requestText, displayText: message.input.displayText, selectedText: message.input.selectedText, attachments: message.input.attachments, references: message.input.references, contextPage: message.input.contextPage }); return <li key={message.id}>
      <span>{index + 1}</span><div className="qy-queued-content">{editing === message.id ? <><textarea aria-label="编辑排队消息" value={text} onChange={(event) => setText(event.target.value)}/><div><button type="button" disabled={!text.trim() && !context.selectedText && !context.attachments.length && !context.references.length} onClick={() => { edit(message.id, { ...message.input, displayText: text.trim(), requestText: requestText(text, context.selectedText, context.page) }); setEditing(null); }}>保存</button><button type="button" onClick={() => setEditing(null)}>取消</button></div></> : <><p>{context.text || context.selectedText || context.references[0]?.title || "附件消息"}</p>{(context.page || context.attachments.length > 0 || context.references.length > 0) && <small>{[context.page?.title, context.references.length ? context.references.length + " 条引用" : "", context.attachments.length ? context.attachments.length + " 个附件" : ""].filter(Boolean).join(" · ")}</small>}</>}</div>
      <button type="button" aria-label={"编辑排队消息 " + (index + 1)} onClick={() => { setText(context.text); setEditing(message.id); }}><Pencil size={13}/></button><button type="button" aria-label={"移除排队消息 " + (index + 1)} onClick={() => remove(message.id)}><X size={14}/></button>
    </li>; })}</ol><small>本次打开期间保留；停止或失败后暂停后续发送。</small>
  </section>;
}
