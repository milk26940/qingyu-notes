import { Check, ChevronDown, Copy, FileText, LoaderCircle, X, AlertCircle } from "lucide-react";
import { memo, useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { usePagesStore } from "../../stores/pagesStore";
import { AgentMarkdown, AttachmentPreview } from "./AgentContent";
import { statusLabel, toolLabel, type TimelineTurn, type TimelineChange } from "./conversationTimeline";

function ChangeReview({ changes, close }: { changes: TimelineChange[]; close: () => void }) {
  const [selected, setSelected] = useState(0);
  const change = changes[selected] ?? changes[0];
  const target = change.after ?? change.before;
  const openPage = usePagesStore((state) => state.open);
  useEffect(() => { const listener = (event: KeyboardEvent) => { if (event.key === "Escape") close(); }; window.addEventListener("keydown", listener); return () => window.removeEventListener("keydown", listener); }, [close]);
  return createPortal(<div className="qy-change-overlay" onClick={close}><section role="dialog" aria-modal="true" aria-label="本轮修改对照" className="qy-change-review" onClick={(event) => event.stopPropagation()}>
    <header><strong>本轮修改</strong><button autoFocus type="button" className="icon-button" aria-label="关闭修改对照" onClick={close}><X size={18}/></button></header>
    <div className="qy-change-tabs">{changes.map((item, index) => <button key={item.id} type="button" aria-pressed={selected === index} onClick={() => setSelected(index)}>{(item.after ?? item.before)?.title ?? item.tool}</button>)}</div>
    <div className="qy-change-heading"><span>{change.confirmed ? "写入已确认" : "写入结果待确认"}</span>{target && <button type="button" onClick={() => { void openPage(target.pageId, change.blockId); close(); }}>打开《{target.title}》 ↗</button>}</div>
    {change.before || change.after ? <div className="qy-change-columns">
      <section><h3>修改前{change.before ? " · " + change.before.title : ""}</h3><pre>{change.before?.content ?? (change.tool === "create" || change.tool === "create_record" ? "（新建内容）" : "未记录修改前内容")}</pre></section>
      <section><h3>修改后{change.after ? " · " + change.after.title : ""}</h3><pre>{change.after?.content ?? (change.tool === "archive" ? "页面已归档" : "未取得修改后快照，请打开页面核对")}</pre></section>
    </div> : <div className="qy-change-legacy"><p>旧记录仅保存了操作摘要，无法提供完整前后对照。</p><pre>{change.preview || "此操作没有可展示的内容快照"}</pre></div>}
  </section></div>, document.body);
}

function Plan({ value }: { value: unknown }) {
  const entries = Array.isArray(value) ? value : value && typeof value === "object" && "plan" in value && Array.isArray(value.plan) ? value.plan : [];
  return entries.length ? <ul className="qy-turn-plan">{entries.map((entry, index) => { const step = typeof entry === "string" ? entry : String(entry?.step ?? entry?.title ?? ""); return <li key={index}>{typeof entry === "object" && entry?.status === "completed" ? "✓ " : "· "}{step}</li>; })}</ul> : null;
}

export const ConversationTurn = memo(function ConversationTurn({ turn, expanded, onToggle }: { turn: TimelineTurn; expanded: boolean; onToggle: () => void }) {
  const openPage = usePagesStore((state) => state.open);
  const [review, setReview] = useState(false);
  const [copied, setCopied] = useState(false);
  const working = turn.status === "inProgress";
  const failed = ["failed", "error"].includes(turn.status);
  const messages = turn.messages.filter((message) => message.text.trim());
  const commentary = messages.filter((message) => message.phase === "commentary");
  const answers = messages.filter((message) => message.phase !== "commentary");
  const tools = turn.tools;
  const current = [...tools].reverse().find((tool) => ["inProgress", "running"].includes(tool.status));
  const duration = turn.endedAt && Number.isFinite(turn.startedAt) ? Math.max(0, Math.round((turn.endedAt - turn.startedAt) / 1000)) : null;
  const changes = turn.changes;
  const confirmed = changes.filter((change) => change.confirmed);
  const pageCount = new Set(confirmed.map((change) => (change.after ?? change.before)?.pageId).filter(Boolean)).size;
  const failedTools = tools.filter((tool) => tool.status === "failed").length;
  const workSummary = [statusLabel(turn.status), tools.length ? tools.length + " 项操作" : "", failedTools ? failedTools + " 项失败" : "", duration !== null ? duration + " 秒" : ""].filter(Boolean).join(" · ");
  return <article className="qy-conversation-turn" aria-label={turn.user?.text ? "对话：" + turn.user.text.slice(0, 40) : "Agent 回复"}>
    {turn.user && <div className="qy-turn-user">
      <div className="qy-turn-context">{turn.user.page && <button type="button" onClick={() => void openPage(turn.user!.page!.id)}><FileText size={12}/>{turn.user.page.title}</button>}{turn.user.selectedText && <details><summary>引用 · {turn.user.selectedText.match(/第\s*\d+(?:\s*[-–至]\s*\d+)?\s*行/)?.[0] ?? "选中文本"}</summary><pre>{turn.user.selectedText}</pre></details>}</div>
      {turn.user.text && <div className="qy-turn-user-text">{turn.user.text}</div>}
      {turn.user.attachments.length > 0 && <div className="qy-turn-attachments">{turn.user.attachments.map((attachment) => <AttachmentPreview key={attachment.id} attachment={attachment}/>)}</div>}
    </div>}
    <div className="qy-turn-assistant">
      {commentary.map((message) => <div key={message.id} className="qy-turn-commentary"><AgentMarkdown text={message.text}/></div>)}
      {(tools.length > 0 || turn.plan != null || working) && <div className="qy-turn-work">
        <button className="qy-turn-work-toggle" type="button" aria-expanded={expanded} onClick={onToggle}>{working ? <LoaderCircle size={14} className="animate-spin"/> : failed ? <AlertCircle size={14}/> : turn.status === "completed" ? <Check size={14}/> : <span>·</span>}<span>{working ? current ? toolLabel(current) : "正在回复" : workSummary}</span><ChevronDown size={13} className={expanded ? "rotate-180" : ""}/></button>
        {expanded && <div className="qy-turn-work-body"><Plan value={turn.plan}/>{tools.map((tool) => <details key={tool.id} className="qy-tool-record"><summary><span>{tool.status === "completed" ? "✓" : tool.status === "failed" ? "!" : "·"}</span><span>{toolLabel(tool)}</span><small>{statusLabel(tool.status)}</small></summary><pre>{[tool.namespace ? tool.namespace + "." + tool.tool : tool.tool, tool.error, tool.arguments ? JSON.stringify(tool.arguments, null, 2) : ""].filter(Boolean).join("\n")}</pre></details>)}</div>}
      </div>}
      {answers.map((message) => <div key={message.id} className="qy-turn-answer"><AgentMarkdown text={message.text}/></div>)}
      {failed && <div className="qy-turn-error" role="alert">请求失败：{turn.error || "本轮未正常完成，已保留收到的内容。"}</div>}
      {turn.status === "interrupted" && <div className="qy-turn-ended">本轮已停止。已完成的写入仍然保留。</div>}
      {!working && turn.status === "completed" && answers.length === 0 && <div className="qy-turn-ended">本轮已结束，未返回最终答复。{confirmed.length ? "可在下方查看已确认的修改。" : ""}</div>}
      {changes.length > 0 && <div className="qy-turn-changes"><button type="button" onClick={() => setReview(true)}><FileText size={14}/><span>{confirmed.length ? "本轮修改" : "变更预览"}{pageCount ? " · " + pageCount + " 页" : ""} · {changes.length} 项操作</span><span>查看修改</span></button><div>{Array.from(new Map(confirmed.map((change) => { const target = change.after ?? change.before; return [target?.pageId, target]; })).values()).filter(Boolean).map((target) => <button key={target!.pageId} type="button" onClick={() => void openPage(target!.pageId)}>{target!.title} ↗</button>)}</div></div>}
      {!working && answers.length > 0 && <button type="button" className="qy-turn-copy" aria-label="复制本轮答复" onClick={() => { void navigator.clipboard.writeText(answers.map((message) => message.text).join("\n\n")).then(() => setCopied(true)).catch(() => setCopied(false)); }}>{copied ? <Check size={13}/> : <Copy size={13}/>} {copied ? "已复制" : "复制"}</button>}
    </div>
    {review && <ChangeReview changes={changes} close={() => setReview(false)}/>}
  </article>;
});
