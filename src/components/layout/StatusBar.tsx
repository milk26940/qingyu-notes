import { CloudOff, Database, Save } from "lucide-react";
import { useCodexStore } from "../../stores/codexStore";

export function StatusBar() {
  const status = useCodexStore((state) => state.status);
  const codexLabel = status.state === "ready" ? "Codex 已连接" : status.state === "starting" ? "正在连接本机 Codex" : status.state === "failed" ? "Codex 连接失败" : "Codex 未连接";
  return (
    <footer className="flex items-center justify-end gap-4 border-t border-[var(--qy-border)] bg-white px-3 text-[11px] text-[var(--qy-muted)]">
      <span className="inline-flex items-center gap-1"><Save size={12} /> 未保存</span>
      <span className="inline-flex items-center gap-1"><Database size={12} /> 索引未启动</span>
      <span className="inline-flex items-center gap-1">{status.state === "ready" ? <CloudOff size={12} className="text-emerald-600" /> : <CloudOff size={12} />} {codexLabel}</span>
    </footer>
  );
}
