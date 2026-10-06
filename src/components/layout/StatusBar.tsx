import { Archive, CloudOff, Database, Link2, LoaderCircle, Save } from "lucide-react";
import { useCodexStore } from "../../stores/codexStore";
import { usePagesStore } from "../../stores/pagesStore";
import { useBackupStore } from "../../stores/backupStore";

export function StatusBar() {
  const status = useCodexStore((state) => state.status);
  const restartRequired = useCodexStore((state) => state.restartRequired);
  const restart = useCodexStore((state) => state.restart);
  const saveState = usePagesStore((state) => state.saveState);
  const projectionStatus = usePagesStore((state) => state.projectionStatus);
  const backupLoading = useBackupStore((state) => state.loading);
  const backupError = useBackupStore((state) => state.error);
  const codexLabel = status.state === "ready" ? "Codex 已连接" : status.state === "starting" ? "正在连接本机 Codex" : status.state === "failed" ? "Codex 连接失败" : "Codex 未连接";
  const saveLabel = saveState === "dirty" ? "等待保存" : saveState === "saving" ? "保存中" : saveState === "saved" ? "已保存" : saveState === "error" ? "保存失败" : saveState === "conflict" ? "保存冲突" : "保存空闲";
  const backupLabel = backupLoading ? "备份进行中" : backupError ? "备份失败" : "备份空闲";
  const projectionLabel = projectionStatus.state === "ready" ? "搜索就绪" : projectionStatus.state === "failed" ? "搜索索引失败" : "搜索索引构建中";
  return (
    <footer className="qy-status-bar flex items-center justify-end gap-4 border-t border-[var(--qy-border)] bg-transparent px-3 text-[11px] font-medium text-[var(--qy-muted)]">
      <span className="inline-flex items-center gap-1"><Save size={12} /> {saveLabel}</span>
      <span className="hidden items-center gap-1 sm:inline-flex"><Archive size={12} /> {backupLabel}</span>
      <span className="inline-flex items-center gap-1"><Database size={12} /> {projectionLabel}</span>
      <span className="inline-flex items-center gap-1">{status.state === "ready" ? <Link2 size={12} className="text-emerald-600" aria-hidden="true" /> : status.state === "starting" ? <LoaderCircle size={12} className="animate-spin" aria-hidden="true" /> : <CloudOff size={12} aria-hidden="true" />} {codexLabel}</span>
      {restartRequired && <button type="button" className="text-[var(--qy-accent)] hover:underline" onClick={() => void restart()}>配置已变化，重启 Agent</button>}
    </footer>
  );
}
