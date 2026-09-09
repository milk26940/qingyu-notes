import * as Dialog from "@radix-ui/react-dialog";
import { ArchiveRestore, Database, Download, FileArchive, FileText, LoaderCircle, Package, Search, Upload, X } from "lucide-react";
import { useEffect, useState } from "react";
import { useBackupStore } from "../../stores/backupStore";
import { usePagesStore } from "../../stores/pagesStore";

const time = (value: string) => new Date(Number(value) * 1000).toLocaleString("zh-CN");

const actionClass = "group flex min-h-20 items-center gap-3 rounded-[18px] border border-[#d7dbe0] bg-white px-4 text-left shadow-[0_3px_12px_rgba(22,28,36,.05)] hover:-translate-y-0.5 hover:border-[#b9c0c9] hover:shadow-[0_8px_20px_rgba(22,28,36,.09)] disabled:translate-y-0";

export function BackupDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const activePage = usePagesStore((state) => state.activePage);
  const {
    backups, backupLocation, loading, error, exportedPath, importReport, importPreview, previewPath,
    load, checkpoint, restore, exportLossless, exportMarkdown, previewContent, importContent,
  } = useBackupStore();
  const [restoreId, setRestoreId] = useState<string | null>(null);
  const [importPath, setImportPath] = useState("");
  useEffect(() => { if (open) void load(); }, [load, open]);
  const previewMatches = importPreview && previewPath === importPath.trim();

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/25 backdrop-blur-[2px]" />
        <Dialog.Content className="fixed left-1/2 top-1/2 z-50 flex max-h-[86vh] w-[min(760px,calc(100vw-32px))] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden rounded-[24px] border border-white bg-[#f7f8fa] shadow-[0_28px_80px_rgba(22,28,36,.24)]">
          <header className="flex shrink-0 items-center justify-between bg-white px-6 py-5">
            <div>
              <Dialog.Title className="text-[18px] font-bold text-[var(--qy-ink)]">备份与恢复</Dialog.Title>
              <p className="mt-1 text-[12px] text-[var(--qy-muted)]">管理本地快照、完整导出和内容迁移</p>
            </div>
            <Dialog.Close className="icon-button h-9 w-9 rounded-xl" aria-label="关闭备份"><X size={18} /></Dialog.Close>
          </header>

          <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">
            <section aria-labelledby="backup-actions-title">
              <div className="mb-3 flex items-center justify-between">
                <h2 id="backup-actions-title" className="text-[13px] font-bold text-[var(--qy-ink)]">创建与导出</h2>
                {loading && <span className="inline-flex items-center gap-1.5 text-[11px] text-[var(--qy-muted)]"><LoaderCircle className="animate-spin" size={14} />处理中</span>}
              </div>
              <div className="grid grid-cols-2 gap-3">
                <button className={actionClass} disabled={loading} onClick={() => void checkpoint({ kind: "vault" })}>
                  <span className="grid h-10 w-10 shrink-0 place-items-center rounded-[14px] bg-[#e9f0ff] text-[#2468e8]"><Database size={19} /></span>
                  <span><strong className="block text-[13px] text-[var(--qy-ink)]">整库备份</strong><small className="mt-1 block text-[11px] text-[var(--qy-muted)]">保存页面、数据库和附件</small></span>
                </button>
                <button className={actionClass} disabled={loading || !activePage} onClick={() => activePage && void checkpoint({ kind: "page", pageId: activePage.id })}>
                  <span className="grid h-10 w-10 shrink-0 place-items-center rounded-[14px] bg-[#eef0f3] text-[#4d5560]"><FileText size={19} /></span>
                  <span><strong className="block text-[13px] text-[var(--qy-ink)]">当前页备份</strong><small className="mt-1 block text-[11px] text-[var(--qy-muted)]">为正在编辑的页面建快照</small></span>
                </button>
                <button className={actionClass} disabled={loading} onClick={() => void exportLossless()}>
                  <span className="grid h-10 w-10 shrink-0 place-items-center rounded-[14px] bg-[#edf7f1] text-[#217a4d]"><Package size={19} /></span>
                  <span><strong className="block text-[13px] text-[var(--qy-ink)]">导出无损包</strong><small className="mt-1 block text-[11px] text-[var(--qy-muted)]">完整迁移全部轻羽数据</small></span>
                </button>
                <button className={actionClass} disabled={loading} onClick={() => void exportMarkdown()}>
                  <span className="grid h-10 w-10 shrink-0 place-items-center rounded-[14px] bg-[#f5f0ff] text-[#6b4cc2]"><Download size={19} /></span>
                  <span><strong className="block text-[13px] text-[var(--qy-ink)]">导出 Markdown</strong><small className="mt-1 block text-[11px] text-[var(--qy-muted)]">生成通用 ZIP 文件</small></span>
                </button>
              </div>
            </section>

            <section className="mt-6" aria-labelledby="snapshot-title">
              <div className="mb-2 flex items-center justify-between gap-2">
                <h2 id="snapshot-title" className="shrink-0 text-[13px] font-bold text-[var(--qy-ink)]">本地快照</h2>
                <span className="shrink-0 text-[11px] text-[var(--qy-muted)]">{backups.length} 个</span>
              </div>
              {backupLocation && (
                <div
                  className="mb-3 flex min-w-0 items-center gap-1 text-[11px] text-[var(--qy-muted)]"
                  title={`备份位置：${backupLocation}`}
                >
                  <span className="shrink-0 font-medium">备份位置：</span>
                  <span className="min-w-0 flex-1 select-all truncate font-mono text-[var(--qy-ink)]">
                    {backupLocation}
                  </span>
                </div>
              )}
              <div className="max-h-52 overflow-y-auto rounded-[18px] border border-[#d7dbe0] bg-white px-4">
                {backups.map((backup) => (
                  <div key={backup.id} className="flex min-h-16 items-center justify-between border-b border-[#e4e7eb] py-3 last:border-b-0">
                    <div className="min-w-0">
                      <div className="truncate text-[12px] font-semibold text-[var(--qy-ink)]">{backup.scope.kind === "vault" ? "整库快照" : "页面快照"} · {backup.entries.length} 个文件</div>
                      <div className="mt-1 text-[11px] text-[var(--qy-muted)]">{time(backup.createdAt)} · {backup.source}</div>
                    </div>
                    {restoreId === backup.id ? (
                      <div className="flex items-center gap-1.5">
                        <button className="h-8 rounded-xl px-3 text-[11px] font-semibold text-[var(--qy-muted)] hover:bg-[#eef0f3]" onClick={() => setRestoreId(null)}>取消</button>
                        <button className="h-8 rounded-xl bg-[#d92d20] px-3 text-[11px] font-semibold text-white hover:bg-[#b42318]" onClick={() => { setRestoreId(null); void restore(backup.id); }}>确认恢复</button>
                      </div>
                    ) : (
                      <button className="icon-button h-9 w-9 shrink-0 rounded-xl" title="恢复此快照" aria-label="恢复此快照" disabled={loading} onClick={() => setRestoreId(backup.id)}><ArchiveRestore size={16} /></button>
                    )}
                  </div>
                ))}
                {!loading && backups.length === 0 && <div className="grid min-h-28 place-items-center text-[12px] text-[var(--qy-muted)]"><span className="inline-flex items-center gap-2"><FileArchive size={16} />暂无本地快照</span></div>}
              </div>
            </section>

            <section className="mt-6" aria-labelledby="import-title">
              <h2 id="import-title" className="mb-3 text-[13px] font-bold text-[var(--qy-ink)]">导入内容</h2>
              <div className="rounded-[18px] border border-[#d7dbe0] bg-white p-4">
                <label className="mb-2 block text-[11px] font-semibold text-[var(--qy-muted)]" htmlFor="backup-import-path">文件、ZIP 或文件夹完整路径</label>
                <div className="flex items-center gap-2">
                  <input id="backup-import-path" className="h-10 min-w-0 flex-1 rounded-[14px] border border-[#cbd0d6] bg-[#fafbfc] px-3 text-[12px] text-[var(--qy-ink)] outline-none focus:border-[#9ba3ad]" value={importPath} onChange={(event) => setImportPath(event.target.value)} placeholder="D:\\资料\\Notion 导出.zip" aria-label="导入文件或目录完整路径" />
                  <button className="inline-flex h-10 shrink-0 items-center gap-2 rounded-[14px] border border-[#cbd0d6] bg-white px-4 text-[12px] font-semibold text-[var(--qy-ink)] hover:bg-[#f0f2f5]" disabled={loading || !importPath.trim()} aria-label="分析导入内容" onClick={() => void previewContent(importPath.trim())}><Search size={15} />分析</button>
                </div>
                {previewMatches && (
                  <div className="mt-3 flex flex-wrap items-center justify-between gap-3 rounded-[14px] bg-[#eef4ff] px-4 py-3">
                    <div>
                      <div className="text-[12px] font-bold text-[#1d2939]">{importPreview.sourceKind}</div>
                      <div className="mt-1 text-[11px] text-[#344054]">预计导入 {importPreview.pages} 个页面、{importPreview.databases} 个数据库，检查 {importPreview.files} 个文件</div>
                      {importPreview.warnings.length > 0 && <div className="mt-1 text-[11px] text-[#b54708]">{importPreview.warnings.join("；")}</div>}
                    </div>
                    <button className="inline-flex h-9 items-center gap-2 rounded-[13px] bg-[#2468e8] px-4 text-[12px] font-semibold text-white shadow-sm hover:bg-[#175cd3]" disabled={loading || importPreview.files === 0} aria-label="确认导入" onClick={() => void importContent(importPath.trim())}><Upload size={15} />确认导入</button>
                  </div>
                )}
              </div>
            </section>

            {exportedPath && <div className="mt-4 truncate text-[11px] text-[var(--qy-muted)]">已导出：{exportedPath}</div>}
            {importReport && <div className="mt-4 text-[11px] text-[#217a4d]">已导入 {importReport.pages} 个页面，检查 {importReport.files} 个文件{importReport.warnings.length ? `；${importReport.warnings.join("；")}` : ""}</div>}
            {error && <div className="mt-4 rounded-[14px] bg-[#fff0ee] px-4 py-3 text-[11px] text-[#b42318]">{error}</div>}
          </div>
          <Dialog.Description className="sr-only">管理轻羽本地快照、导出包和内容导入</Dialog.Description>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
