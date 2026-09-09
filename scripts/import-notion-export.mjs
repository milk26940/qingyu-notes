import { createHash, randomUUID } from "node:crypto";
import { existsSync } from "node:fs";
import { cp, mkdir, readFile, readdir, rename, rm, writeFile } from "node:fs/promises";
import path from "node:path";

const args = process.argv.slice(2);
const dryRun = args.includes("--dry-run");
const positional = args.filter((value) => value !== "--dry-run");
if (positional.length !== 2) {
  throw new Error("用法：node scripts/import-notion-export.mjs <Notion目录> <轻羽vault> [--dry-run]");
}

const sourceRoot = path.resolve(positional[0]);
const vaultRoot = path.resolve(positional[1]);
const rootTitle = path.basename(sourceRoot);
const rootPagePath = path.join(vaultRoot, `${safeName(rootTitle)}.md`);
const rootChildrenPath = path.join(vaultRoot, safeName(rootTitle));
if (!existsSync(sourceRoot)) throw new Error(`Notion 导出目录不存在：${sourceRoot}`);
if (!existsSync(vaultRoot)) throw new Error(`轻羽笔记库不存在：${vaultRoot}`);
if (existsSync(rootPagePath) || existsSync(rootChildrenPath)) {
  throw new Error(`目标页面已经存在，拒绝重复导入：${rootTitle}`);
}

const sourceFiles = await walkFiles(sourceRoot);
const markdownFiles = sourceFiles.filter((file) => path.extname(file).toLowerCase() === ".md");
const databaseFiles = sourceFiles.filter((file) => /_all\.csv$/i.test(file));
const imageFiles = sourceFiles.filter((file) => path.extname(file).toLowerCase() === ".png");
const currentCsvByStem = new Map(
  sourceFiles
    .filter((file) => path.extname(file).toLowerCase() === ".csv" && !/_all\.csv$/i.test(file))
    .map((file) => [notionHex(file), file]),
);

const rootPage = pageRecord({ id: randomUUID(), title: rootTitle, parent: null, source: null });
const pages = [rootPage];
const sourceTargets = new Map();
for (const file of markdownFiles) {
  const page = pageRecord({ id: notionId(file), title: notionTitle(file), parent: null, source: file });
  pages.push(page);
  sourceTargets.set(key(file), page);
}
const databases = [];
for (const file of databaseFiles) {
  const database = pageRecord({ id: notionId(file), title: notionTitle(file), parent: null, source: file, kind: "database" });
  database.rows = parseCsv(await readFile(file, "utf8"));
  databases.push(database);
  pages.push(database);
  sourceTargets.set(key(file), database);
  const current = currentCsvByStem.get(notionHex(file));
  if (current) sourceTargets.set(key(current), database);
}

const directoryOwners = new Map([[key(sourceRoot), rootPage]]);
for (const page of pages.filter((item) => item.source)) {
  const childDirectory = path.join(path.dirname(page.source), page.title);
  if (existsSync(childDirectory)) directoryOwners.set(key(childDirectory), page);
}
for (const page of pages.filter((item) => item.source)) {
  page.parent = directoryOwners.get(key(path.dirname(page.source))) ?? rootPage;
  if (page.parent.kind === "database") {
    page.kind = "databaseRecord";
    page.databaseId = page.parent.id;
  }
}

for (const database of databases) {
  if (database.rows.length === 0) continue;
  const [headers, ...rows] = database.rows;
  database.fields = headers.slice(1).map((name) => ({ id: randomUUID(), name: name || "文本", fieldType: "text", options: [] }));
  const directRecords = pages.filter((page) => page.parent === database && page.kind === "databaseRecord");
  const recordsByTitle = new Map();
  for (const record of directRecords) {
    const queue = recordsByTitle.get(record.title) ?? [];
    queue.push(record);
    recordsByTitle.set(record.title, queue);
  }
  rows.forEach((values, index) => {
    const title = values[0]?.trim() || `未命名记录 ${index + 1}`;
    let record = recordsByTitle.get(title)?.shift();
    if (!record) {
      record = pageRecord({ id: randomUUID(), title, parent: database, source: null, kind: "databaseRecord", databaseId: database.id });
      pages.push(record);
    }
    record.properties = Object.fromEntries(database.fields.map((field, fieldIndex) => [field.id, values[fieldIndex + 1] ?? ""]));
  });
}

const siblings = new Map();
for (const page of pages) {
  const parentId = page.parent?.id ?? "root";
  const list = siblings.get(parentId) ?? [];
  page.sortOrder = list.length;
  list.push(page);
  siblings.set(parentId, list);
}

const targetPaths = new Map();
function targetPath(page) {
  if (targetPaths.has(page.id)) return targetPaths.get(page.id);
  const directory = page.parent ? targetPath(page.parent).replace(/\.md$/i, "") : vaultRoot;
  const used = new Set([...targetPaths.values()].filter((value) => path.dirname(value) === directory).map((value) => path.basename(value).toLowerCase()));
  let fileName = `${safeName(page.title)}.md`;
  for (let suffix = 2; used.has(fileName.toLowerCase()); suffix += 1) fileName = `${safeName(page.title)} (${suffix}).md`;
  const value = path.join(directory, fileName);
  targetPaths.set(page.id, value);
  return value;
}
for (const page of pages) targetPath(page);

const stageRoot = path.join(path.dirname(vaultRoot), `.notion-import-${randomUUID()}`);
const stagedVault = path.join(stageRoot, "vault");
const assetCopies = [];
const unresolvedLinks = [];
const usedImages = new Set();
try {
  for (const page of pages) {
    let content = page.source && path.extname(page.source).toLowerCase() === ".md" ? await readFile(page.source, "utf8") : "";
    content = removeDuplicateTitle(content, page.title);
    content = await rewriteMarkdown(content, page);
    if (page === rootPage) {
      const children = siblings.get(page.id) ?? [];
      content = children.map((child) => `[${child.title}](qingyu://page/${child.id})`).join("\n\n");
    }
    const relative = path.relative(vaultRoot, targetPath(page));
    const stagedPath = path.join(stagedVault, relative);
    await mkdir(path.dirname(stagedPath), { recursive: true });
    await writeFile(stagedPath, serializePage(page, content), "utf8");
  }
  for (const database of databases) {
    const definition = {
      id: database.id,
      pageId: database.id,
      fields: database.fields ?? [],
      views: [{ id: "table-default", name: "表格", viewType: "table", filters: [], sorts: [] }],
      createdAt: timestamp(),
      updatedAt: timestamp(),
    };
    const definitionPath = path.join(stagedVault, ".noteapp", "databases", `${database.id}.json`);
    await mkdir(path.dirname(definitionPath), { recursive: true });
    await writeFile(definitionPath, `${JSON.stringify(definition, null, 2)}\n`, "utf8");
  }
  for (const copy of assetCopies) {
    const destination = path.join(stagedVault, ".noteapp", "assets", copy.pageId, copy.fileName);
    await mkdir(path.dirname(destination), { recursive: true });
    await cp(copy.source, destination, { force: false });
  }
  if (unresolvedLinks.length) {
    throw new Error(`存在 ${unresolvedLinks.length} 个无法解析的本地链接：\n${unresolvedLinks.slice(0, 12).join("\n")}`);
  }
  if (usedImages.size !== imageFiles.length) {
    const unused = imageFiles.filter((file) => !usedImages.has(key(file)));
    throw new Error(`有 ${unused.length} 张图片未被页面引用：\n${unused.slice(0, 12).join("\n")}`);
  }

  const report = {
    mode: dryRun ? "dry-run" : "import",
    rootTitle,
    pages: pages.filter((page) => page.kind === "page").length,
    databases: databases.length,
    databaseRecords: pages.filter((page) => page.kind === "databaseRecord").length,
    images: assetCopies.length,
    unresolvedLinks: unresolvedLinks.length,
  };
  if (!dryRun) await commitStage(stagedVault);
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
} finally {
  await rm(stageRoot, { recursive: true, force: true });
}

async function rewriteMarkdown(markdown, page) {
  let output = "";
  let cursor = 0;
  const pattern = /(!?)\[([^\]]*)\]\(([^)]+)\)/g;
  for (const match of markdown.matchAll(pattern)) {
    const index = match.index ?? 0;
    output += markdown.slice(cursor, index);
    cursor = index + match[0].length;
    const marker = match[1];
    const label = match[2];
    const href = match[3].trim().replace(/^<|>$/g, "");
    if (/^(?:https?:|mailto:|qingyu:|#)/i.test(href)) {
      output += match[0];
      continue;
    }
    const [pathPart, anchor = ""] = href.split("#", 2);
    let decoded;
    try { decoded = decodeURIComponent(pathPart); } catch { decoded = pathPart; }
    const sourceTarget = path.resolve(path.dirname(page.source), decoded);
    if (marker === "!") {
      if (!existsSync(sourceTarget) || path.extname(sourceTarget).toLowerCase() !== ".png") {
        unresolvedLinks.push(`${page.source}: ${href}`);
        output += match[0];
        continue;
      }
      const fileName = `${randomUUID()}.png`;
      assetCopies.push({ source: sourceTarget, pageId: page.id, fileName });
      usedImages.add(key(sourceTarget));
      output += `![${label}](/.noteapp/assets/${page.id}/${fileName})`;
      continue;
    }
    const target = sourceTargets.get(key(sourceTarget));
    if (target) {
      output += `[${label}](qingyu://page/${target.id}${anchor ? `#${anchor}` : ""})`;
    } else if (existsSync(sourceTarget) || /\.(?:md|csv)$/i.test(decoded)) {
      unresolvedLinks.push(`${page.source}: ${href}`);
      output += match[0];
    } else {
      output += match[0];
    }
  }
  return output + markdown.slice(cursor);
}

async function commitStage(staged) {
  const moves = [];
  const roots = await readdir(staged, { withFileTypes: true });
  try {
    for (const entry of roots.filter((item) => item.name !== ".noteapp")) {
      const source = path.join(staged, entry.name);
      const destination = path.join(vaultRoot, entry.name);
      if (existsSync(destination)) throw new Error(`目标已存在：${destination}`);
      await rename(source, destination);
      moves.push(destination);
    }
    const noteapp = path.join(staged, ".noteapp");
    for (const category of ["assets", "databases"]) {
      const sourceCategory = path.join(noteapp, category);
      if (!existsSync(sourceCategory)) continue;
      await mkdir(path.join(vaultRoot, ".noteapp", category), { recursive: true });
      for (const entry of await readdir(sourceCategory)) {
        const source = path.join(sourceCategory, entry);
        const destination = path.join(vaultRoot, ".noteapp", category, entry);
        if (existsSync(destination)) throw new Error(`目标已存在：${destination}`);
        await rename(source, destination);
        moves.push(destination);
      }
    }
  } catch (error) {
    for (const destination of moves.reverse()) await rm(destination, { recursive: true, force: true });
    throw error;
  }
}

function pageRecord({ id, title, parent, source, kind = "page", databaseId = null }) {
  return { id, title, parent, source, kind, databaseId, properties: {}, sortOrder: 0 };
}

function serializePage(page, content) {
  const now = timestamp();
  return `---\nid: ${page.id}\ntitle: ${page.title.replaceAll("\n", " ")}\nparent_id: ${page.parent?.id ?? ""}\nkind: ${page.kind}\ncreated_at: ${now}\nupdated_at: ${now}\ntrashed_at: \nsort_order: ${page.sortOrder}\ndatabase_id: ${page.databaseId ?? ""}\nproperties: ${JSON.stringify(page.properties)}\n---\n${content}`;
}

function removeDuplicateTitle(markdown, title) {
  const normalized = markdown.replace(/^\uFEFF/, "");
  const match = normalized.match(/^#\s+(.+?)\s*(?:\r?\n|$)/);
  return match && match[1].trim() === title.trim() ? normalized.slice(match[0].length).replace(/^\r?\n/, "") : normalized;
}

function notionTitle(file) {
  return path.basename(file, path.extname(file)).replace(/_all$/i, "").replace(/\s+[0-9a-f]{32}$/i, "").trim();
}

function notionHex(file) {
  const match = path.basename(file, path.extname(file)).replace(/_all$/i, "").match(/([0-9a-f]{32})$/i);
  if (!match) throw new Error(`文件名缺少 Notion ID：${file}`);
  return match[1].toLowerCase();
}

function notionId(file) {
  const value = notionHex(file);
  return `${value.slice(0, 8)}-${value.slice(8, 12)}-${value.slice(12, 16)}-${value.slice(16, 20)}-${value.slice(20)}`;
}

function parseCsv(text) {
  const rows = [];
  let row = [];
  let field = "";
  let quoted = false;
  for (let index = 0; index < text.length; index += 1) {
    const char = text[index];
    if (quoted) {
      if (char === '"' && text[index + 1] === '"') { field += '"'; index += 1; }
      else if (char === '"') quoted = false;
      else field += char;
    } else if (char === '"') quoted = true;
    else if (char === ",") { row.push(field); field = ""; }
    else if (char === "\n") { row.push(field.replace(/\r$/, "")); rows.push(row); row = []; field = ""; }
    else field += char;
  }
  if (field || row.length) { row.push(field.replace(/\r$/, "")); rows.push(row); }
  return rows.filter((values) => values.some((value) => value.length));
}

async function walkFiles(root) {
  const output = [];
  for (const entry of await readdir(root, { withFileTypes: true })) {
    const value = path.join(root, entry.name);
    if (entry.isDirectory()) output.push(...await walkFiles(value));
    else if (entry.isFile()) output.push(value);
  }
  return output;
}

function safeName(value) {
  const cleaned = value.replace(/[\\/:*?"<>|\u0000-\u001f]/g, "_").trim().replace(/^\.+|\.+$/g, "");
  return [...(cleaned || "未命名页面")].slice(0, 80).join("");
}

function key(value) { return path.resolve(value).toLowerCase(); }
function timestamp() { return String(Math.floor(Date.now() / 1000)); }
function digest(value) { return createHash("sha256").update(value).digest("hex"); }
void digest;
