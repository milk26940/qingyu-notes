import { describe, expect, it } from "vitest";
import { findImageDeletionRange } from "./imageDeletion";

function remove(markdown: string, position: number) {
  const range = findImageDeletionRange(markdown, position);
  return range ? markdown.slice(0, range.from) + markdown.slice(range.to) : markdown;
}

describe("findImageDeletionRange", () => {
  it("删除单独图片行及其换行", () => {
    const markdown = "正文\n\n![截图](/.noteapp/assets/page-1/image.png)\n\n下一段";
    const position = markdown.indexOf("![截图") + 3;
    expect(remove(markdown, position)).toBe("正文\n\n下一段");
  });

  it("删除图片和同一行的块标记", () => {
    const markdown = "![截图](/.noteapp/assets/page-1/image.png) <!-- qy:block:11111111-1111-4111-8111-111111111111 -->\n下一段";
    expect(remove(markdown, 4)).toBe("下一段");
  });

  it("删除图片及相邻独立块标记", () => {
    const markdown = "<!-- qy:block:11111111-1111-4111-8111-111111111111 -->\n![截图](/.noteapp/assets/page-1/image.png)\n<!-- qy:block:22222222-2222-4222-8222-222222222222 -->\n下一段";
    expect(remove(markdown, markdown.indexOf("![截图") + 2)).toBe("下一段");
  });

  it("正文混排时只删除图片语法，不吞掉正文", () => {
    const markdown = "开头 ![截图](/.noteapp/assets/page-1/image.png) 结尾";
    expect(remove(markdown, markdown.indexOf("![截图") + 2)).toBe("开头  结尾");
  });
});
