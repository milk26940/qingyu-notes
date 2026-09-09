import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { usePagesStore } from "../../stores/pagesStore";
import { IndexProgress, SettingField, SettingNotice, inputClass, primaryClass, secondaryClass } from "./SettingsUI";

type Config = { baseUrl: string; model: string; enabled: boolean; hasApiKey: boolean; concurrency?: number };
type Status = { running: boolean; units: number; conflicts: { title: string; pageId: string; draft: string; currentContent?: string | null }[]; progress?: { total: number; done: number; title: string; active?: number; concurrency?: number } };
const failure = (error: unknown) => error && typeof error === "object" && "message" in error ? String(error.message) : String(error);

export function KnowledgeSettings({ view }: { view: "config" | "index" }) {
  const [config, setConfig] = useState<Config>({ baseUrl: "", model: "", enabled: false, hasApiKey: false });
  const [key, setKey] = useState("");
  const [concurrency, setConcurrency] = useState("6");
  const [concurrencyMessage, setConcurrencyMessage] = useState("");
  const [starting, setStarting] = useState(false);
  const [saving, setSaving] = useState(false);
  const [configMessage, setConfigMessage] = useState("");
  const [message, setMessage] = useState("");
  const [status, setStatus] = useState<Status | null>(null);
  const busy = starting || Boolean(status?.running);
  const refresh = () => invoke<Status>("knowledge_status").then(setStatus).catch((e) => setMessage(failure(e)));
  useEffect(() => {
    void invoke<Config>("knowledge_get_config").then((value) => { setConfig(value); setConcurrency(String(value.concurrency ?? 6)); }).catch((e) => setConfigMessage(failure(e)));
    void refresh();
    const timer = setInterval(() => void refresh(), 2000);
    return () => clearInterval(timer);
  }, []);
  const save = async () => {
    setSaving(true);
    try { await invoke("knowledge_save_config", { baseUrl: config.baseUrl.trim(), model: config.model.trim(), enabled: config.enabled, apiKey: key || null }); setKey(""); setConfig((value) => ({ ...value, hasApiKey: value.hasApiKey || Boolean(key) })); setConfigMessage("知识整理模型已保存"); }
    catch (e) { setConfigMessage(failure(e)); } finally { setSaving(false); }
  };
  const rebuild = async (full: boolean) => {
    setStarting(true); setMessage("");
    try {
      const result = await invoke<{ updated: number; reused: number; failed: number; errors: string[]; cancelled: boolean; conflicts: number }>("knowledge_rebuild", { full });
      setMessage(`${result.cancelled ? "已停止" : "更新完成"}：更新 ${result.updated}，复用 ${result.reused}，失败 ${result.failed}\n${result.errors.join("\n")}`);
      await refresh(); await usePagesStore.getState().refresh();
    } catch (e) { setMessage(failure(e)); } finally { setStarting(false); }
  };
  return <>
    <div hidden={view !== "config"}>
      <p className="mb-5 text-sm leading-6 text-[#515b69]">用于生成 Wiki 和提取知识关系。支持图片输入的对话模型可同时整理图片。</p>
      <fieldset disabled={busy || saving} className="space-y-4 disabled:opacity-70">
        <SettingField label="接口地址"><input className={inputClass} placeholder="https://…/v1" value={config.baseUrl} onChange={(e) => setConfig({ ...config, baseUrl: e.target.value })} /></SettingField>
        <SettingField label="API Key"><input type="password" className={inputClass} autoComplete="off" placeholder={config.hasApiKey ? "已保存，留空保留" : "填写 API Key"} value={key} onChange={(e) => setKey(e.target.value)} /></SettingField>
        <SettingField label="模型名称"><input className={inputClass} value={config.model} onChange={(e) => setConfig({ ...config, model: e.target.value })} /></SettingField>
        <label className="flex items-center gap-2 text-sm font-medium"><input type="checkbox" className="accent-[#2463eb]" checked={config.enabled} onChange={(e) => setConfig({ ...config, enabled: e.target.checked })} />启用知识整理</label>
      </fieldset>
      {busy && <p className="mt-4 text-sm text-[#515b69]">任务正在运行，可在“索引管理”查看进度。</p>}
      <SettingNotice text={configMessage} />
      <div className="mt-6 flex justify-end"><button disabled={busy || saving} className={primaryClass} onClick={() => void save()}>{saving ? "保存中…" : "保存配置"}</button></div>
    </div>
    <section hidden={view !== "index"} className="rounded-2xl border border-[#dfe3e9] bg-white p-5">
      <div className="flex items-start justify-between gap-3"><div><h3 className="text-base font-semibold text-[#202630]">Wiki 与知识关系</h3><p className="mt-1 text-sm text-[#596372]">整理主题、概念和来源关系</p></div><span className="shrink-0 rounded-lg bg-[#f0f3f7] px-2.5 py-1 text-xs font-medium text-[#465366]">{busy ? `更新中 · ${status?.progress?.active ?? 0}/${status?.progress?.concurrency ?? 6} 并发` : "已整理 " + (status?.units ?? 0) + " 个片段"}</span></div>
      {busy && status?.progress && <IndexProgress {...status.progress} />}
      <form className="mt-4 flex flex-wrap items-center gap-3" onSubmit={(e) => { e.preventDefault(); const value=Number(concurrency); if (!Number.isInteger(value) || value<1 || value>32) {setConcurrencyMessage("请输入 1–32 的整数");return;} void invoke("knowledge_set_concurrency",{value}).then(()=>setConcurrencyMessage("已保存，运行中的任务也会应用")).catch((e)=>setConcurrencyMessage(failure(e))); }}>
        <label className="flex items-center gap-3 text-sm font-medium">并发数量<input type="number" min="1" max="32" step="1" value={concurrency} onChange={(e)=>setConcurrency(e.target.value)} className="h-9 w-20 rounded-xl border border-[#d9dde3] px-3" /></label>
        <button className={secondaryClass} type="submit">应用</button><span className="text-xs text-[#596372]">1–32，默认 6</span>
      </form>
      {concurrencyMessage && <p role="status" className="mt-2 text-xs text-[#596372]">{concurrencyMessage}</p>}
      {busy && <p className="mt-3 text-xs text-[#596372]">关闭此窗口后任务继续运行</p>}
      <SettingNotice text={message} />
      <div className="mt-5 flex items-center justify-between gap-3">
        <details className="relative"><summary className="cursor-pointer list-none text-sm font-medium text-[#596372]">更多操作 ···</summary><div className="absolute bottom-full left-0 z-10 mb-2 w-40 rounded-xl border border-[#dfe3e9] bg-white p-1 shadow-lg"><button disabled={busy || !config.enabled} className="w-full rounded-lg px-3 py-2 text-left text-sm hover:bg-[#f0f3f7] disabled:opacity-50" onClick={(e) => { e.currentTarget.closest("details")?.removeAttribute("open"); void rebuild(true); }}>完整重建</button></div></details>
        {busy ? <button className={secondaryClass} onClick={() => void invoke("knowledge_cancel").then(() => setMessage("正在停止，等待当前请求结束")).catch((e) => setMessage(failure(e)))}>停止更新</button> : <button disabled={!config.enabled || !config.hasApiKey} className={primaryClass} onClick={() => void rebuild(false)}>更新知识</button>}
      </div>
      {status?.conflicts.length ? <details className="mt-4 border-t border-[#dfe3e9] pt-3"><summary className="cursor-pointer text-sm font-medium">{status.conflicts.length} 篇页面有待处理草稿</summary>{status.conflicts.map((item) => <details key={item.pageId} className="mt-3 rounded-xl bg-[#f5f6f8] p-3"><summary className="cursor-pointer text-sm font-semibold">{item.title}</summary><div className="mt-3 space-y-3 text-sm"><div><p className="font-semibold">当前内容</p><pre className="mt-1 max-h-36 overflow-auto whitespace-pre-wrap">{item.currentContent ?? "页面已删除"}</pre></div><div><p className="font-semibold">新草稿</p><pre className="mt-1 max-h-36 overflow-auto whitespace-pre-wrap">{item.draft}</pre></div><div className="flex gap-2">{item.currentContent != null && <button className={secondaryClass} onClick={() => void invoke("knowledge_apply_draft", { pageId: item.pageId, expectedContent: item.currentContent }).then(() => refresh()).catch((e) => setMessage(failure(e)))}>采用新草稿</button>}<button className={secondaryClass} onClick={() => void usePagesStore.getState().open(item.pageId)}>打开原页面</button></div></div></details>)}</details> : null}
    </section>
  </>;
}
