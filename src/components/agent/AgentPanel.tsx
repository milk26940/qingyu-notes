import { useEffect } from "react";
import { Bot, LoaderCircle, PanelRightClose, RotateCw } from "lucide-react";
import { useCodexStore } from "../../stores/codexStore";
import { useWorkspaceStore } from "../../stores/workspaceStore";

export function AgentPanel() {
  const status = useCodexStore((state) => state.status);
  const connect = useCodexStore((state) => state.connect);
  const subscribe = useCodexStore((state) => state.subscribe);
  const toggleAgentPanel = useWorkspaceStore((state) => state.toggleAgentPanel);
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void subscribe().then((cleanup) => { unlisten = cleanup; }).catch(() => undefined);
    void connect();
    return () => unlisten?.();
  }, [connect, subscribe]);
  const ready = status.state === "ready";
  const message = status.state === "starting" ? "正在连接本机 Codex" : status.state === "failed" ? status.message : ready ? "Codex 已连接" : "未找到本机 Codex";
  return (
    <aside className="flex min-h-0 flex-col bg-[var(--qy-sidebar)]" aria-label="Agent 面板">
      <div className="flex h-10 items-center justify-between border-b border-[var(--qy-border)] px-3"><div className="flex items-center gap-2 text-[13px] font-medium"><Bot size={16} /> Agent</div><button className="icon-button" title="关闭 Agent 面板" aria-label="关闭 Agent 面板" onClick={toggleAgentPanel}><PanelRightClose size={16} /></button></div>
      <div className="flex flex-1 flex-col items-center justify-center px-6 text-center text-[13px] text-[var(--qy-muted)]">
        {status.state === "starting" ? <LoaderCircle className="mb-3 animate-spin" size={22} /> : <Bot className="mb-3 text-[var(--qy-faint)]" size={24} strokeWidth={1.5} />}
        <p>{message}</p>
        {ready && <p className="mt-2 break-all text-[11px] text-[var(--qy-faint)]">{status.codexHome}</p>}
        {status.state === "failed" && <button type="button" className="mt-4 inline-flex items-center gap-1 text-[12px] text-[var(--qy-accent)]" onClick={() => void connect()}><RotateCw size={13} />重新连接</button>}
      </div>
      <div className="border-t border-[var(--qy-border)] p-3"><textarea disabled className="h-20 w-full resize-none rounded-md border border-[var(--qy-border)] bg-white p-2 text-[12px] outline-none" placeholder="页面系统完成后即可开始对话" /></div>
    </aside>
  );
}
