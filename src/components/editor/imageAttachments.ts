import { storePageAsset } from "../../lib/tauri/pages";

const IMAGE_TYPES = new Set(["image/png", "image/jpeg", "image/gif", "image/webp"]);

export function supportedImageFiles(files: FileList | File[]) {
  return Array.from(files).filter((file) => IMAGE_TYPES.has(file.type));
}

export function clipboardImageFiles(data: DataTransfer) {
  const files = supportedImageFiles(data.files);
  if (files.length) return files;
  return Array.from(data.items)
    .filter((item) => item.kind === "file" && IMAGE_TYPES.has(item.type))
    .map((item) => item.getAsFile())
    .filter((file): file is File => Boolean(file));
}

function imageAlt(fileName: string) {
  return fileName.replace(/\.[^.]+$/, "").replace(/[\[\]]/g, "").trim() || "图片";
}

export async function storeImageAttachment(pageId: string, file: File) {
  if (!IMAGE_TYPES.has(file.type)) throw new Error("仅支持 PNG、JPEG、GIF 和 WebP");
  if (file.size > 20 * 1024 * 1024) throw new Error("图片不能超过 20 MiB");
  const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
  const fallbackName = file.type === "image/jpeg" ? "粘贴图片.jpg" : file.type === "image/webp" ? "粘贴图片.webp" : file.type === "image/gif" ? "粘贴图片.gif" : "粘贴图片.png";
  const name = file.name || fallbackName;
  const path = await storePageAsset(pageId, name, bytes);
  return `![${imageAlt(name)}](${path})`;
}

export async function storeEditorAttachment(pageId: string, file: File) {
  if (file.size > 100 * 1024 * 1024) throw new Error("附件不能超过 100 MiB");
  const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
  const name = file.name || "附件";
  const path = await storePageAsset(pageId, name, bytes);
  return { path, name, image: IMAGE_TYPES.has(file.type), mime: file.type };
}
