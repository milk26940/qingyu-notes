import { storePageAsset } from "../../lib/tauri/pages";

const IMAGE_TYPES = new Set(["image/png", "image/jpeg", "image/gif", "image/webp"]);

export function supportedImageFiles(files: FileList | File[]) {
  return Array.from(files).filter((file) => IMAGE_TYPES.has(file.type));
}

function imageAlt(fileName: string) {
  return fileName.replace(/\.[^.]+$/, "").replace(/[\[\]]/g, "").trim() || "图片";
}

export async function storeImageAttachment(pageId: string, file: File) {
  if (!IMAGE_TYPES.has(file.type)) throw new Error("仅支持 PNG、JPEG、GIF 和 WebP");
  if (file.size > 20 * 1024 * 1024) throw new Error("图片不能超过 20 MiB");
  const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
  const path = await storePageAsset(pageId, file.name, bytes);
  return `![${imageAlt(file.name)}](${path})`;
}
