import { Archive, Check, Feather, PanelLeft, PanelRight, Save, Trash2, X } from "lucide-react";
import type { PageTab } from "./WorkspaceShell";
import { useEffect, useRef, useState } from "react";
import { useWorkspaceStore } from "../../stores/workspaceStore";
import { BackupDialog } from "../backup/BackupDialog";
import { usePagesStore } from "../../stores/pagesStore";
import { DeleteConfirmDialog, countDescendants } from "../pages/DeleteConfirmDialog";

export function Topbar({
  tabs = [],
  activeTabId,
  onSelectTab,
  onCloseTab,
  onCloseTabs,
}: {
  tabs?: PageTab[];
  activeTabId?: string | null;
  onSelectTab?: (id: string) => void;
  onCloseTab?: (id: string) => void;
  onCloseTabs?: (ids: string[]) => void;
}) {
  const { toggleSidebar, toggleAgentPanel } = useWorkspaceStore();
  const activePage = usePagesStore((state) => state.activePage);
  const saveState = usePagesStore((state) => state.saveState);
  const save = usePagesStore((state) => state.save);
  const pages = usePagesStore((state) => state.pages);
  const trashPageRecursive = usePagesStore((state) => state.trashPageRecursive);
  const [backupOpen, setBackupOpen] = useState(false);
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);
  const [tabMenu, setTabMenu] = useState<{
    pageId: string;
    x: number;
    y: number;
  } | null>(null);
  const tabMenuRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!tabMenu) return;
    const close = (event: PointerEvent) => {
      if (!tabMenuRef.current?.contains(event.target as Node)) setTabMenu(null);
    };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, [tabMenu]);
  const menuIndex = tabMenu
    ? tabs.findIndex((tab) => tab.pageId === tabMenu.pageId)
    : -1;
  const closeMany = (ids: string[]) => {
    setTabMenu(null);
    onCloseTabs?.(ids);
  };
  const removeActivePage = () => {
    if (!activePage) return;
    setDeleteConfirmOpen(true);
  };
  return (
    <header className="relative z-40 grid min-w-0 grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3 overflow-hidden rounded-[14px] border border-white bg-white px-4 shadow-[var(--qy-panel-shadow)]">
      <div aria-label="轻羽品牌" className="flex min-w-0 items-center gap-3 text-[var(--qy-ink)]">
        <span aria-label="轻羽应用图标" className="grid h-8 w-10 place-items-center rounded-xl border border-[#c9cbd0] bg-[#f7f7f5] text-[#55585e] shadow-sm">
          <Feather size={23} strokeWidth={1.45} aria-hidden="true" />
        </span>
        <span className="hidden text-[15px] font-bold sm:inline">轻羽</span>
      </div>
      <div className="min-w-0 overflow-hidden">
        <div className="qy-tabs-scroll flex min-w-0 items-center gap-1 overflow-x-auto px-2">
        {tabs.length === 0 && (
          <span className="px-2 text-[12px] text-[var(--qy-faint)]">
            打开一个页面开始编辑
          </span>
        )}
        {tabs.map((tab) => (
          <div
            key={tab.pageId}
            onContextMenu={(event) => {
              event.preventDefault();
              setTabMenu({
                pageId: tab.pageId,
                x: event.clientX,
                y: event.clientY,
              });
            }}
            className={`group flex h-8 shrink-0 items-center gap-1 rounded-xl px-3 text-[12px] font-medium ${tab.pageId === activeTabId ? "bg-[var(--qy-selected)] text-[var(--qy-ink)] shadow-sm" : "text-[var(--qy-muted)] hover:bg-[var(--qy-bg)]"}`}
          >
            <button
              type="button"
              className="max-w-40 truncate"
              onClick={() => onSelectTab?.(tab.pageId)}
            >
              {tab.title}
            </button>
            <button
              type="button"
              aria-label={`关闭${tab.title}`}
              title="关闭标签"
              className="opacity-60 hover:opacity-100"
              onClick={() => onCloseTab?.(tab.pageId)}
            >
              <X size={12} />
            </button>
          </div>
        ))}
        </div>
      </div>
      {tabMenu && menuIndex >= 0 && (
        <div
          ref={tabMenuRef}
          className="fixed z-50 min-w-40 rounded-2xl border border-[var(--qy-border)] bg-white p-1.5 text-[12px] shadow-[var(--qy-float-shadow)]"
          style={{ left: tabMenu.x, top: tabMenu.y }}
        >
          <button
            type="button"
            className="block w-full rounded px-2 py-1.5 text-left hover:bg-[var(--qy-bg)]"
            onClick={() => closeMany([tabMenu.pageId])}
          >
            关闭当前标签页
          </button>
          {menuIndex > 0 && (
            <button
              type="button"
              className="block w-full rounded px-2 py-1.5 text-left hover:bg-[var(--qy-bg)]"
              onClick={() =>
                closeMany(tabs.slice(0, menuIndex).map((tab) => tab.pageId))
              }
            >
              关闭左侧标签页
            </button>
          )}
          {menuIndex < tabs.length - 1 && (
            <button
              type="button"
              className="block w-full rounded px-2 py-1.5 text-left hover:bg-[var(--qy-bg)]"
              onClick={() =>
                closeMany(tabs.slice(menuIndex + 1).map((tab) => tab.pageId))
              }
            >
              关闭右侧标签页
            </button>
          )}
          {tabs.length > 1 && (
            <button
              type="button"
              className="block w-full rounded px-2 py-1.5 text-left hover:bg-[var(--qy-bg)]"
              onClick={() =>
                closeMany(
                  tabs
                    .filter((tab) => tab.pageId !== tabMenu.pageId)
                    .map((tab) => tab.pageId),
                )
              }
            >
              关闭其他标签页
            </button>
          )}
          <button
            type="button"
            className="block w-full rounded px-2 py-1.5 text-left hover:bg-[var(--qy-bg)]"
            onClick={() => closeMany(tabs.map((tab) => tab.pageId))}
          >
            全部关闭
          </button>
        </div>
      )}
      <div className="flex min-w-0 shrink-0 items-center justify-end gap-1 border-l border-[var(--qy-border)] pl-2">
        <button
          className="icon-button"
          title="切换侧栏"
          aria-label="切换侧栏"
          onClick={toggleSidebar}
        >
          <PanelLeft size={16} />
        </button>
        <span className="hidden px-2 text-[13px] font-medium text-[var(--qy-muted)] lg:inline">
          我的笔记库
        </span>
        {activePage && (
          <button
            className="icon-button"
            title="保存页面"
            aria-label="保存页面"
            onClick={() => void save()}
          >
            <Save size={16} />
          </button>
        )}
        {activePage && <button className="icon-button" title="递归删除页面" aria-label="递归删除页面" onClick={removeActivePage}><Trash2 size={16} /></button>}
        <button
          className="icon-button"
          title="备份与恢复"
          aria-label="备份与恢复"
          onClick={() => setBackupOpen(true)}
        >
          <Archive size={16} />
        </button>
        {activePage && (
          <span className="flex items-center gap-1 px-1 text-[11px] text-[var(--qy-faint)]">
            {saveState === "saved" && (
              <>
                <Check size={12} />
                已保存
              </>
            )}
            {saveState === "dirty" && "未保存"}
            {saveState === "saving" && "保存中"}
          </span>
        )}
        <button
          className="icon-button"
          title="切换 Agent 面板"
          aria-label="切换 Agent 面板"
          onClick={toggleAgentPanel}
        >
          <PanelRight size={16} />
        </button>
      </div>
      <BackupDialog open={backupOpen} onOpenChange={setBackupOpen} />
      {activePage && (
        <DeleteConfirmDialog
          open={deleteConfirmOpen}
          onOpenChange={setDeleteConfirmOpen}
          title={activePage.title}
          kind={activePage.kind}
          descendantCount={countDescendants(activePage.id, pages)}
          onConfirm={async () => {
            await trashPageRecursive(activePage.id);
            setDeleteConfirmOpen(false);
          }}
        />
      )}
    </header>
  );
}
