import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Page } from "../lib/tauri/pages";
import * as api from "../lib/tauri/pages";
import { usePagesStore } from "./pagesStore";

vi.mock("../lib/tauri/pages", () => ({
  initVault: vi.fn(),
  listPages: vi.fn(),
  getPage: vi.fn(),
  openPageBundle: vi.fn(),
  savePageDocument: vi.fn(),
  createPage: vi.fn(),
  updatePage: vi.fn(),
  movePage: vi.fn(),
  trashPage: vi.fn(),
  restorePage: vi.fn(),
  purgePage: vi.fn(),
  searchPages: vi.fn(),
  getPageLinks: vi.fn(),
  getPageBacklinks: vi.fn(),
  getPageHistory: vi.fn(),
}));

const page = (id: string, content: string): Page => ({
  id,
  title: `页面 ${id}`,
  parentId: null,
  kind: "page",
  content,
  createdAt: "1",
  updatedAt: "1",
  trashedAt: null,
  sortOrder: 0,
  properties: {},
  databaseId: null,
});

describe("页面自动保存", () => {
  beforeEach(async () => {
    vi.useFakeTimers();
    vi.mocked(api.listPages).mockResolvedValue([]);
    vi.mocked(api.getPageLinks).mockResolvedValue([]);
    vi.mocked(api.getPageBacklinks).mockResolvedValue([]);
    vi.mocked(api.getPageHistory).mockResolvedValue([]);
    await usePagesStore.getState().refresh();
    usePagesStore.setState({
      vaultPath: "C:\\notes",
      pages: [],
      trash: [],
      activePage: page("a", "旧内容"),
      persistedPage: page("a", "旧内容"),
      targetBlockId: null,
      links: [],
      backlinks: [],
      history: [],
      searchResults: [],
      searchQuery: "",
      saveState: "idle",
      error: null,
    });
  });

  afterEach(() => {
    vi.clearAllTimers();
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it("三分钟后自动保存正文并进入已保存状态", async () => {
    vi.mocked(api.updatePage).mockImplementation(async (id, title, content) => ({ ...page(id, content), title }));

    usePagesStore.getState().edit("新内容");
    await vi.advanceTimersByTimeAsync(180_000);

    expect(api.updatePage).toHaveBeenCalledWith("a", "页面 a", "新内容", "页面 a", "旧内容");
    expect(usePagesStore.getState().saveState).toBe("saved");
  });

  it("切换页面前先保存当前未提交内容", async () => {
    const second = page("b", "第二页");
    vi.mocked(api.updatePage).mockImplementation(async (id, title, content) => ({ ...page(id, content), title }));
    vi.mocked(api.openPageBundle).mockResolvedValue({ page: second, document: { pageId: "b", title: second.title, blocks: [], revision: 0 }, links: [], backlinks: [] });

    usePagesStore.getState().edit("离开前内容");
    await usePagesStore.getState().open("b");

    expect(api.updatePage).toHaveBeenCalledWith("a", "页面 a", "离开前内容", "页面 a", "旧内容");
    expect(api.updatePage).toHaveBeenCalledBefore(vi.mocked(api.openPageBundle));
    expect(usePagesStore.getState().activePage?.id).toBe("b");
  });

  it("页面正文和关系通过单次 bundle 查询一起载入", async () => {
    const second = page("b", "第二页");
    vi.mocked(api.openPageBundle).mockResolvedValue({ page: second, document: { pageId: "b", title: second.title, blocks: [{ id: "block-b", type: "paragraph", text: "第二页", attrs: {}, children: [] }], revision: 0 }, links: [], backlinks: [] });

    await usePagesStore.getState().open("b");

    expect(api.openPageBundle).toHaveBeenCalledTimes(1);
    expect(usePagesStore.getState().activePage?.id).toBe("b");
    expect(usePagesStore.getState().activeDocument?.blocks[0]?.id).toBe("block-b");
  });

  it("忽略仍等于保存基线的文件系统事件", async () => {
    vi.mocked(api.getPage).mockResolvedValue(page("a", "旧内容"));
    usePagesStore.getState().edit("继续输入");

    await usePagesStore.getState().handleExternalChange({
      source: "external",
      namespace: "filesystem",
      tool: "change",
      paths: ["页面 a.md"],
      pageIds: [],
      databaseIds: [],
    });

    expect(usePagesStore.getState().saveState).toBe("dirty");
    expect(usePagesStore.getState().activePage?.content).toBe("继续输入");
  });

  it("磁盘内容偏离保存基线时保留缓冲并标记冲突", async () => {
    vi.mocked(api.getPage).mockResolvedValue(page("a", "外部内容"));
    usePagesStore.getState().edit("本地内容");

    await usePagesStore.getState().handleExternalChange({
      source: "external",
      namespace: "filesystem",
      tool: "change",
      paths: ["页面 a.md"],
      pageIds: [],
      databaseIds: [],
    });

    expect(usePagesStore.getState().saveState).toBe("conflict");
    expect(usePagesStore.getState().activePage?.content).toBe("本地内容");
  });

  it("切换笔记库前保存当前页面", async () => {
    vi.mocked(api.updatePage).mockImplementation(async (id, title, content) => ({ ...page(id, content), title }));
    vi.mocked(api.initVault).mockResolvedValue("D:\\新笔记库");
    usePagesStore.getState().edit("切库前内容");

    await usePagesStore.getState().init("D:\\新笔记库");

    expect(api.updatePage).toHaveBeenCalledWith("a", "页面 a", "切库前内容", "页面 a", "旧内容");
    expect(api.updatePage).toHaveBeenCalledBefore(vi.mocked(api.initVault));
    expect(usePagesStore.getState().vaultPath).toBe("D:\\新笔记库");
  });

  it("冲突后可以丢弃本地缓冲并重载磁盘版本", async () => {
    vi.mocked(api.getPage).mockResolvedValue(page("a", "磁盘版本"));
    usePagesStore.setState({ activePage: page("a", "本地版本"), saveState: "conflict" });

    await usePagesStore.getState().discardConflictAndReload();

    expect(usePagesStore.getState().activePage?.content).toBe("磁盘版本");
    expect(usePagesStore.getState().saveState).toBe("idle");
  });

  it("冲突后可以把本地缓冲保留为副本", async () => {
    const local = page("a", "本地版本");
    vi.mocked(api.createPage).mockResolvedValue({ ...local, id: "copy", title: "页面 a (本地副本)" });
    usePagesStore.setState({ activePage: local, saveState: "conflict" });

    await usePagesStore.getState().keepConflictAsCopy();

    expect(api.createPage).toHaveBeenCalledWith("页面 a (本地副本)", null, "本地版本");
    expect(usePagesStore.getState().activePage?.id).toBe("copy");
    expect(usePagesStore.getState().saveState).toBe("saved");
  });
});
