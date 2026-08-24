export type QingyuPageLink = { pageId: string; blockId: string | null };

export function parseQingyuPageLink(value: string): QingyuPageLink | null {
  try {
    const url = new URL(value);
    if (url.protocol !== "qingyu:" || url.hostname !== "page") return null;
    const pageId = decodeURIComponent(url.pathname.replace(/^\//, "")).trim();
    if (!pageId || pageId.includes("/")) return null;
    const blockId = url.hash ? decodeURIComponent(url.hash.slice(1)).trim() || null : null;
    return { pageId, blockId };
  } catch {
    return null;
  }
}
