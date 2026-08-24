import { WorkspaceShell } from "../components/layout/WorkspaceShell";
import { AgentPanel } from "../components/agent/AgentPanel";
import { MarkdownEditor } from "../components/editor/MarkdownEditor";
import { PageToolbar } from "../components/pages/PageToolbar";
import { useEffect } from "react";
import { usePagesStore } from "../stores/pagesStore";
import { DatabaseRecordProperties, DatabaseTable } from "../components/database/DatabaseTable";
import { subscribeVaultChanges } from "../lib/tauri/pages";
import { useDatabaseStore } from "../stores/databaseStore";
import { useAppearanceStore } from "../stores/appearanceStore";
import { PageChildren } from "../components/pages/PageChildren";

function App() {
  const { activePage, init, edit, editTitle, searchQuery, searchResults, open } = usePagesStore();
  const applyFontScale = useAppearanceStore((state) => state.applyFontScale);
  useEffect(() => { void init(); }, [init]);
  useEffect(() => {
    applyFontScale();
    const handleFontShortcut = (event: KeyboardEvent) => {
      if (!event.ctrlKey || event.altKey || event.isComposing) return;
      const appearance = useAppearanceStore.getState();
      if (event.key === "+" || event.key === "=") appearance.increaseFont();
      else if (event.key === "-") appearance.decreaseFont();
      else if (event.key === "0") appearance.resetFont();
      else return;
      event.preventDefault();
    };
    window.addEventListener("keydown", handleFontShortcut);
    return () => window.removeEventListener("keydown", handleFontShortcut);
  }, [applyFontScale]);
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void subscribeVaultChanges((change) => {
      void usePagesStore.getState().handleExternalChange(change);
      void useDatabaseStore.getState().refreshExternalChange();
    }).then((cleanup) => { if (disposed) cleanup(); else unlisten = cleanup; }).catch(() => undefined);
    return () => { disposed = true; unlisten?.(); };
  }, []);
  return (
    <WorkspaceShell agentPanel={<AgentPanel />}>
      <div className="flex h-full flex-col">
        <PageToolbar />
        {searchQuery && <div className="border-b border-[var(--qy-border)] bg-[var(--qy-bg)] px-5 py-2">{searchResults.length ? searchResults.map((result) => <button type="button" key={result.page.id} onClick={() => void open(result.page.id)} className="block w-full truncate py-1 text-left text-[12px] text-[var(--qy-muted)] hover:text-[var(--qy-ink)]"><span className="font-medium">{result.page.title}</span><span className="ml-2 text-[var(--qy-faint)]">{result.snippet}</span></button>) : <div className="py-1 text-[12px] text-[var(--qy-faint)]">没有匹配页面</div>}</div>}
        {activePage?.kind === "database" ? <DatabaseTable databaseId={activePage.id} /> : activePage ? <div className="flex min-h-0 flex-1 flex-col px-8 py-6"><input aria-label="页面标题" value={activePage.title} onChange={(event) => editTitle(event.target.value)} className="mb-4 border-0 text-xl font-semibold outline-none" />{activePage.kind === "databaseRecord" && <DatabaseRecordProperties record={activePage} />}<PageChildren pageId={activePage.id} /><MarkdownEditor value={activePage.content} onChange={edit} /></div> : <div className="grid flex-1 place-items-center text-center"><div><h1 className="text-xl font-semibold">开始构建你的知识库</h1><p className="mt-2 text-sm text-[var(--qy-muted)]">从左侧新建一个 Markdown 页面</p></div></div>}
      </div>
    </WorkspaceShell>
  );
}

export default App;
