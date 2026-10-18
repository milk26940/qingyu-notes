import { describe, expect, it, vi } from "vitest";
import { generateJSON } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import TaskList from "@tiptap/extension-task-list";
import TaskItem from "@tiptap/extension-task-item";
import type { PageDocument } from "../../lib/tauri/pages";
import { documentToEditorContent, documentToTiptap, hydrateEditorContent, markdownToHtml, selectionWithLines, tiptapToDocument } from "./blockDocument";

describe("区块文档转换", () => {
  it("Markdown 待办生成任务节点并保留完成状态和正文", () => {
    const html = markdownToHtml("- [x] 已完成\n- [ ] 待处理");
    const json = generateJSON(html, [StarterKit, TaskList, TaskItem]);
    expect(json.content?.[0]).toMatchObject({ type: "taskList", content: [
      { type: "taskItem", attrs: { checked: true }, content: [{ type: "paragraph", content: [{ type: "text", text: "已完成" }] }] },
      { type: "taskItem", attrs: { checked: false }, content: [{ type: "paragraph", content: [{ type: "text", text: "待处理" }] }] },
    ] });
  });
  it("保留稳定区块 ID 和页面链接节点", () => {
    const document: PageDocument = {
      pageId: "page-1",
      title: "标题",
      revision: 3,
      blocks: [{ id: "block-1", type: "paragraph", text: "目标页面", attrs: { node: { type: "paragraph", content: [{ type: "text", text: "目标页面", marks: [{ type: "link", attrs: { href: "qingyu://page/page-2" } }] }] } }, children: [] }],
    };

    const tiptap = documentToTiptap(document);
    const restored = tiptapToDocument(tiptap, document);

    expect(restored.blocks[0].id).toBe("block-1");
    expect(restored.blocks[0].text).toBe("目标页面");
    expect(JSON.stringify(restored.blocks[0].attrs)).toContain("qingyu://page/page-2");
  });

  it("展开旧数据中嵌套的 Tiptap doc 区块", () => {
    const document: PageDocument = {
      pageId: "legacy-rich",
      title: "旧页面",
      revision: 1,
      blocks: [{
        id: "wrapper",
        type: "paragraph",
        text: "正文",
        attrs: { node: { type: "doc", content: [{ type: "paragraph", content: [{ type: "text", text: "正文" }] }] } },
        children: [],
      }],
    };

    const tiptap = documentToTiptap(document);
    expect(tiptap.content?.map((node) => node.type)).toEqual(["paragraph"]);
    expect(tiptap.content?.[0].type).not.toBe("doc");
  });

  it("旧纯文本区块转换后不暴露 Markdown 标记", () => {
    const document: PageDocument = { pageId: "page-1", title: "标题", revision: 0, blocks: [{ id: "heading-1", type: "heading", text: "一级标题", attrs: { level: 1 }, children: [] }] };
    const tiptap = documentToTiptap(document);
    expect(JSON.stringify(tiptap)).not.toContain("# 一级标题");
    expect(tiptap.content?.[0].type).toBe("heading");
  });

  it("旧 Markdown 页面在首次打开时转换为富文本 HTML", () => {
    const document: PageDocument = { pageId: "legacy", title: "旧页面", revision: 0, blocks: [{ id: "legacy-block", type: "markdown", text: "# 标题\n\n<aside>\n提示\n</aside>\n\n```mermaid\ngraph LR\nA --> B\n```", attrs: {}, children: [] }] };
    const content = documentToEditorContent(document);
    expect(content).toBeTypeOf("string");
    expect(content).toContain("<h1>标题</h1>");
    expect(content).toContain("<blockquote>");
    expect(content).toContain("language-mermaid");
  });

  it("Markdown 导入的区块标记转换为不可见定位点", () => {
    const document: PageDocument = { pageId: "imported", title: "导入页面", revision: 0, blocks: [{ id: "legacy", type: "markdown", text: "<!-- qingyu-block:section-1 -->\n## 章节", attrs: {}, children: [] }] };
    const content = documentToEditorContent(document);
    expect(content).toBeTypeOf("string");
    expect(content).toContain('data-block-id="section-1"');
    expect(content).toContain("<h2>章节</h2>");
  });

  it("添加选区到对话时附带起止行号", () => {
    expect(selectionWithLines("第一行\n第二行", "第三行\n第四行")).toBe("第 2-3 行\n第三行\n第四行");
    expect(selectionWithLines("", "第一行")).toBe("第 1 行\n第一行");
  });

  it("Markdown HTML 路径：三张 /.noteapp/assets 图片中一张读取失败时另外两张仍成功水合，且保留原始路径与失败提示", async () => {
    const mockReader = vi.fn(async (path: string) => {
      if (path === "/.noteapp/assets/page-1/img1.png") return { mime: "image/png", bytes: [137, 80, 78, 71] };
      if (path === "/.noteapp/assets/page-1/img2.png") throw new Error("读取磁盘失败");
      if (path === "/.noteapp/assets/page-1/img3.png") return { mime: "image/jpeg", bytes: [255, 216, 255, 224] };
      throw new Error("未知路径");
    });

    const document: PageDocument = {
      pageId: "page-1",
      title: "Notion 导入页面",
      revision: 1,
      blocks: [{
        id: "md-1",
        type: "markdown",
        text: "![图片一](/.noteapp/assets/page-1/img1.png)\n\n![图片二](/.noteapp/assets/page-1/img2.png)\n\n![图片三](/.noteapp/assets/page-1/img3.png)",
        attrs: {},
        children: [],
      }],
    };

    const initialContent = documentToEditorContent(document);
    expect(typeof initialContent).toBe("string");

    const hydratedHtml = (await hydrateEditorContent(initialContent, mockReader)) as string;
    expect(typeof hydratedHtml).toBe("string");

    // 图片 1 成功水合为 base64，title 记录原始路径
    expect(hydratedHtml).toContain('src="data:image/png;base64,iVBORw=="');
    expect(hydratedHtml).toContain('title="/.noteapp/assets/page-1/img1.png"');
    expect(hydratedHtml).toContain('alt="图片一"');

    // 图片 2 读取失败不阻断整体，保持 title 并显示失败提示
    expect(hydratedHtml).toContain('alt="图片二（加载失败）"');
    expect(hydratedHtml).toContain('title="/.noteapp/assets/page-1/img2.png"');

    // 图片 3 成功水合为 base64，title 记录原始路径
    expect(hydratedHtml).toContain('src="data:image/jpeg;base64,/9j/4A=="');
    expect(hydratedHtml).toContain('title="/.noteapp/assets/page-1/img3.png"');
    expect(hydratedHtml).toContain('alt="图片三"');
  });

  it("结构化 JSON 路径：三张 /.noteapp/assets 图片中一张读取失败时另外两张仍成功水合，并在保存时还原原始路径", async () => {
    const mockReader = vi.fn(async (path: string) => {
      if (path === "/.noteapp/assets/page-1/img1.png") return { mime: "image/png", bytes: [137, 80, 78, 71] };
      if (path === "/.noteapp/assets/page-1/img2.png") throw new Error("读取磁盘失败");
      if (path === "/.noteapp/assets/page-1/img3.png") return { mime: "image/jpeg", bytes: [255, 216, 255, 224] };
      throw new Error("未知路径");
    });

    const document: PageDocument = {
      pageId: "page-1",
      title: "结构化页面",
      revision: 1,
      blocks: [
        { id: "b-1", type: "image", text: "", attrs: { node: { type: "image", attrs: { src: "/.noteapp/assets/page-1/img1.png", alt: "图片一" } } }, children: [] },
        { id: "b-2", type: "image", text: "", attrs: { node: { type: "image", attrs: { src: "/.noteapp/assets/page-1/img2.png", alt: "图片二" } } }, children: [] },
        { id: "b-3", type: "image", text: "", attrs: { node: { type: "image", attrs: { src: "/.noteapp/assets/page-1/img3.png", alt: "图片三" } } }, children: [] },
      ],
    };

    const initialContent = documentToEditorContent(document);
    expect(typeof initialContent).toBe("object");

    const hydratedJson = (await hydrateEditorContent(initialContent, mockReader)) as any;
    expect(hydratedJson.content).toHaveLength(3);

    // 图片 1 成功
    expect(hydratedJson.content[0].attrs.src).toBe("data:image/png;base64,iVBORw==");
    expect(hydratedJson.content[0].attrs.title).toBe("/.noteapp/assets/page-1/img1.png");
    expect(hydratedJson.content[0].attrs.alt).toBe("图片一");

    // 图片 2 失败不阻断其他图片，保留原始 title 与加载失败提示
    expect(hydratedJson.content[1].attrs.alt).toBe("图片二（加载失败）");
    expect(hydratedJson.content[1].attrs.title).toBe("/.noteapp/assets/page-1/img2.png");
    expect(hydratedJson.content[1].attrs.src).toBe("/.noteapp/assets/page-1/img2.png");

    // 图片 3 成功
    expect(hydratedJson.content[2].attrs.src).toBe("data:image/jpeg;base64,/9j/4A==");
    expect(hydratedJson.content[2].attrs.title).toBe("/.noteapp/assets/page-1/img3.png");
    expect(hydratedJson.content[2].attrs.alt).toBe("图片三");

    // 保存还原：通过 title 恢复原始路径，不将 base64 存入文档
    const restoredDocument = tiptapToDocument(hydratedJson, document);
    expect((restoredDocument.blocks[0].attrs.node as any)?.attrs?.src).toBe("/.noteapp/assets/page-1/img1.png");
    expect((restoredDocument.blocks[1].attrs.node as any)?.attrs?.src).toBe("/.noteapp/assets/page-1/img2.png");
    expect((restoredDocument.blocks[2].attrs.node as any)?.attrs?.src).toBe("/.noteapp/assets/page-1/img3.png");
    expect(JSON.stringify(restoredDocument)).not.toContain("data:image");
  });
});
