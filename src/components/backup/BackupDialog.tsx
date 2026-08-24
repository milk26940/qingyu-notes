import * as Dialog from "@radix-ui/react-dialog";
import { ArchiveRestore, Download, FileArchive, LoaderCircle, Upload, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useBackupStore } from "../../stores/backupStore";
import { usePagesStore } from "../../stores/pagesStore";

const time = (value: string) => new Date(Number(value) * 1000).toLocaleString("zh-CN");

export function BackupDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const activePage = usePagesStore((state) => state.activePage);
  const { backups, loading, error, exportedPath, load, checkpoint, restore, exportZip, importZip } = useBackupStore();
  const [restoreId, setRestoreId] = useState<string | null>(null);
  const [importPath, setImportPath] = useState("");
  useEffect(() => { if (open) void load(); }, [load, open]);

  return <Dialog.Root open={open} onOpenChange={onOpenChange}>
    <Dialog.Portal>
      <Dialog.Overlay className="fixed inset-0 z-40 bg-black/20" />
      <Dialog.Content className="fixed left-1/2 top-1/2 z-50 flex max-h-[78vh] w-[min(680px,calc(100vw-32px))] -translate-x-1/2 -translate-y-1/2 flex-col rounded-md border border-[var(--qy-border)] bg-white shadow-xl">
        <div className="flex h-11 shrink-0 items-center justify-between border-b border-[var(--qy-border)] px-4"><Dialog.Title className="text-[14px] font-semibold">备份与恢复</Dialog.Title><Dialog.Close className="icon-button" aria-label="关闭备份"><X size={16}/></Dialog.Close></div>
        <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-[var(--qy-border)] px-4 py-3">
          <button className="inline-flex h-8 items-center gap-1 rounded-md bg-[var(--qy-accent)] px-3 text-[12px] text-white" disabled={loading} onClick={() => void checkpoint({ kind: "vault" })}><FileArchive size={14}/>整库备份</button>
          <button className="inline-flex h-8 items-center gap-1 rounded-md border border-[var(--qy-border)] px-3 text-[12px]" disabled={loading || !activePage} onClick={() => activePage && void checkpoint({ kind: "page", pageId: activePage.id })}><FileArchive size={14}/>当前页备份</button>
          <button className="inline-flex h-8 items-center gap-1 rounded-md border border-[var(--qy-border)] px-3 text-[12px]" disabled={loading} onClick={() => void exportZip()}><Download size={14}/>导出 ZIP</button>
          {loading && <LoaderCircle className="animate-spin text-[var(--qy-muted)]" size={15}/>} 
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-4">
          {backups.map((backup) => <div key={backup.id} className="flex min-h-12 items-center justify-between border-b border-[var(--qy-border)] py-2"><div className="min-w-0"><div className="truncate text-[12px] text-[var(--qy-ink)]">{backup.scope.kind === "vault" ? "整库快照" : "页面快照"} · {backup.entries.length} 个文件</div><div className="mt-1 text-[10px] text-[var(--qy-faint)]">{time(backup.createdAt)} · {backup.source}</div></div>{restoreId === backup.id ? <div className="flex items-center gap-1"><button className="h-7 rounded-md px-2 text-[11px] text-[var(--qy-muted)]" onClick={() => setRestoreId(null)}>取消</button><button className="h-7 rounded-md bg-red-600 px-2 text-[11px] text-white" onClick={() => { setRestoreId(null); void restore(backup.id); }}>确认恢复</button></div> : <button className="icon-button shrink-0" title="恢复此快照" aria-label="恢复此快照" disabled={loading} onClick={() => setRestoreId(backup.id)}><ArchiveRestore size={15}/></button>}</div>)}
          {!loading && backups.length === 0 && <div className="py-10 text-center text-[12px] text-[var(--qy-faint)]">暂无备份</div>}
        </div>
        <div className="shrink-0 border-t border-[var(--qy-border)] px-4 py-3"><div className="flex items-center gap-2"><input className="h-8 min-w-0 flex-1 rounded-md border border-[var(--qy-border)] px-2 text-[11px] outline-none" value={importPath} onChange={(event) => setImportPath(event.target.value)} placeholder="ZIP 文件完整路径" aria-label="ZIP 文件完整路径"/><button className="inline-flex h-8 shrink-0 items-center gap-1 rounded-md border border-[var(--qy-border)] px-3 text-[12px]" disabled={loading || !importPath.trim()} onClick={() => void importZip(importPath.trim())}><Upload size={14}/>导入</button></div>{exportedPath && <div className="mt-2 truncate text-[10px] text-[var(--qy-faint)]">已导出：{exportedPath}</div>}{error && <div className="mt-2 text-[11px] text-red-600">{error}</div>}</div>
        <Dialog.Description className="sr-only">管理轻羽本地快照和 ZIP 备份</Dialog.Description>
      </Dialog.Content>
    </Dialog.Portal>
  </Dialog.Root>;
}
