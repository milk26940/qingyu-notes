import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { X } from "lucide-react";
import { getSemanticConfig, rebuildSemanticIndex, saveSemanticConfig, setSemanticConcurrency, testSemanticConnection } from "../../lib/tauri/semantic";
import { KnowledgeSettings } from "./KnowledgeSettings";
import { IndexProgress, SettingField, SettingNotice, inputClass, primaryClass, secondaryClass } from "./SettingsUI";

function errorText(error: unknown) { return error && typeof error === "object" && "message" in error ? String(error.message) : String(error); }
export function SettingsDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const [tab, setTab] = useState<"models" | "index">("models");
  const [provider, setProvider] = useState<"embedding" | "knowledge">("embedding");
  const [baseUrl, setBaseUrl] = useState("");
  const [model, setModel] = useState("qwen3-vl-embedding");
  const [dimension, setDimension] = useState("1024");
  const [apiKey, setApiKey] = useState("");
  const [enabled, setEnabled] = useState(false);
  const [hasKey, setHasKey] = useState(false);
  const [concurrency, setConcurrency] = useState("6");
  const [concurrencyMessage, setConcurrencyMessage] = useState("");
  const [configMessage, setConfigMessage] = useState("");
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState(false);
  const [working, setWorking] = useState(false);
  const [progress, setProgress] = useState<{total:number;done:number;title:string;active?:number;concurrency?:number} | null>(null);
  useEffect(() => {
    if (!open) return;
    void getSemanticConfig().then((value) => { setBaseUrl(value.baseUrl); setModel(value.model); setDimension(String(value.dimension)); setEnabled(value.enabled); setHasKey(value.hasApiKey); setConcurrency(String(value.concurrency ?? 6)); }).catch((e) => setConfigMessage(errorText(e)));
    const escape = (e: KeyboardEvent) => { if (e.key === "Escape") onOpenChange(false); };
    document.addEventListener("keydown", escape);
    return () => document.removeEventListener("keydown", escape);
  }, [open, onOpenChange]);
  useEffect(() => {
    if (!open || !busy) return;
    const timer = setInterval(() => { void invoke<{embeddingProgress?:{total:number;done:number;title:string;active?:number;concurrency?:number}}>("knowledge_status").then((value) => setProgress(value.embeddingProgress ?? null)).catch(() => undefined); }, 1500);
    return () => clearInterval(timer);
  }, [open, busy]);
  const save = async () => { setWorking(true); try { const value = await saveSemanticConfig({ baseUrl, model, dimension: Number(dimension), enabled, apiKey }); setHasKey(value.hasApiKey); setApiKey(""); setConfigMessage("语义检索模型已保存"); } catch (e) { setConfigMessage(errorText(e)); } finally { setWorking(false); } };
  const test = async () => { setWorking(true); try { const value = await testSemanticConnection({ baseUrl, model, dimension: Number(dimension), apiKey }); setConfigMessage(`连接成功，返回 ${value} 维向量`); } catch (e) { setConfigMessage(errorText(e)); } finally { setWorking(false); } };
  const rebuild = async (full = false) => { setBusy(true); setMessage(""); setProgress(null); try { const s = await rebuildSemanticIndex(full); setMessage(`${s.cancelled ? "已停止" : "更新完成"}：更新 ${s.updated}，复用 ${s.reused}，失败 ${s.failed}\n${(s.errors ?? []).join("\n")}`); } catch (e) { setMessage(errorText(e)); } finally { setBusy(false); } };
  const preview = async () => { setWorking(true); try { const d = await invoke<{total:number;pending:number;reused:number;images:number;errors:string[]}>("semantic_preview"); setMessage(`待更新 ${d.pending}，可复用 ${d.reused}，共 ${d.total} 个片段\n${d.errors.join("\n")}`); } catch(e) { setMessage(errorText(e)); } finally { setWorking(false); } };
  const saveConcurrency = async () => {
    const value = Number(concurrency);
    if (!Number.isInteger(value) || value < 1 || value > 32) { setConcurrencyMessage("请输入 1–32 的整数"); return; }
    try { await setSemanticConcurrency(value); setConcurrencyMessage("已保存，运行中的任务也会应用"); } catch (e) { setConcurrencyMessage(errorText(e)); }
  };
  if (!open) return null;
  return <div className="fixed inset-0 z-[100] grid place-items-center bg-black/20 p-4 backdrop-blur-[3px]" onMouseDown={(e) => { if (e.target === e.currentTarget) onOpenChange(false); }}>
    <section className="flex max-h-[min(780px,90vh)] w-full max-w-[660px] flex-col overflow-hidden rounded-[24px] border border-[#e0e3e9] bg-white text-[#252b35] shadow-[0_24px_80px_#18223826]" role="dialog" aria-modal="true" aria-labelledby="knowledge-settings-title">
      <header className="shrink-0 border-b border-[#e6e8ed] px-6 pt-5">
        <div className="flex items-center justify-between"><h2 id="knowledge-settings-title" className="text-xl font-semibold">知识设置</h2><button className="icon-button" aria-label="关闭设置" onClick={() => onOpenChange(false)}><X size={19} /></button></div>
        <nav className="mt-4 flex gap-6" aria-label="设置分类">{(["models","index"] as const).map((item) => <button key={item} type="button" aria-current={tab===item ? "page" : undefined} className={"border-b-2 px-1 pb-3 text-sm font-semibold transition-colors duration-200 " + (tab===item ? "border-[#2463eb] text-[#2463eb]" : "border-transparent text-[#616b7a] hover:text-[#252b35]")} onClick={() => setTab(item)}>{item==="models" ? "模型配置" : "索引管理"}</button>)}</nav>
      </header>
      <div className="min-h-0 overflow-y-auto px-6 py-5">
        <div hidden={tab!=="models"}>
          <div className="mb-5 flex rounded-xl bg-[#f0f2f5] p-1">{(["embedding","knowledge"] as const).map((item) => <button key={item} className={"flex-1 rounded-lg px-3 py-2 text-sm font-semibold transition-colors duration-200 " + (provider===item ? "bg-white text-[#252b35] shadow-sm" : "text-[#626d7a]")} onClick={() => setProvider(item)}>{item==="embedding" ? "语义检索模型" : "知识整理模型"}</button>)}</div>
        </div>
        <div hidden={tab!=="models" || provider!=="embedding"}>
          <p className="mb-5 text-sm leading-6 text-[#515b69]">用于理解文本和图片，查找语义相近的笔记内容。</p>
          <fieldset disabled={working || busy} className="space-y-4 disabled:opacity-70">
            <SettingField label="接口地址"><input className={inputClass} value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)} placeholder="https://dashscope.aliyuncs.com/api/v1" /></SettingField>
            <SettingField label="API Key"><input type="password" autoComplete="off" className={inputClass} value={apiKey} onChange={(e) => setApiKey(e.target.value)} placeholder={hasKey ? "已保存，留空保留" : "填写 API Key"} /></SettingField>
            <div className="grid grid-cols-[minmax(0,1fr)_110px] gap-3"><SettingField label="模型名称"><input className={inputClass} value={model} onChange={(e) => setModel(e.target.value)} /></SettingField><SettingField label="向量维度"><input type="number" min="1" max="2560" className={inputClass} value={dimension} onChange={(e) => setDimension(e.target.value)} /></SettingField></div>
            <label className="flex items-center gap-2 text-sm font-medium"><input type="checkbox" className="accent-[#2463eb]" checked={enabled} onChange={(e) => setEnabled(e.target.checked)} />启用语义检索</label>
          </fieldset>
          <SettingNotice text={configMessage} />
          <div className="mt-6 flex justify-end gap-2"><button disabled={working || busy} className={secondaryClass} onClick={() => void test()}>测试连接</button><button disabled={working || busy} className={primaryClass} onClick={() => void save()}>{working ? "处理中…" : "保存配置"}</button></div>
        </div>
        <div hidden={tab!=="index"}>
          <p className="mb-4 text-sm leading-6 text-[#596372]">仅手动更新，未修改的内容会自动复用。</p>
          <section className="mb-4 rounded-2xl border border-[#dfe3e9] bg-white p-5">
            <div className="flex items-start justify-between gap-3"><div><h3 className="text-base font-semibold">语义索引</h3><p className="mt-1 text-sm text-[#596372]">文本、图片与附件的检索索引</p></div><span className="shrink-0 rounded-lg bg-[#f0f3f7] px-2.5 py-1 text-xs text-[#465366]">{busy ? `更新中 · ${progress?.active ?? 0}/${progress?.concurrency ?? (Number(concurrency) || 6)} 并发` : "手动更新"}</span></div>
            {busy && progress && <IndexProgress {...progress} />}
            <form className="mt-4 flex flex-wrap items-center gap-3" onSubmit={(e) => { e.preventDefault(); void saveConcurrency(); }}>
              <label className="flex items-center gap-3 text-sm font-medium">并发数量<input type="number" min="1" max="32" step="1" value={concurrency} onChange={(e) => setConcurrency(e.target.value)} className="h-9 w-20 rounded-xl border border-[#d9dde3] px-3" /></label>
              <button className={secondaryClass} type="submit">应用</button><span className="text-xs text-[#596372]">1–32，默认 6</span>
            </form>
            {concurrencyMessage && <p role="status" className="mt-2 text-xs text-[#596372]">{concurrencyMessage}</p>}
            <SettingNotice text={message} />
            <div className="mt-5 flex items-center justify-between gap-3"><details className="relative"><summary className="cursor-pointer list-none text-sm font-medium text-[#596372]">更多操作 ···</summary><div className="absolute bottom-full left-0 z-10 mb-2 w-40 rounded-xl border border-[#dfe3e9] bg-white p-1 shadow-lg"><button disabled={busy || working} className="w-full rounded-lg px-3 py-2 text-left text-sm hover:bg-[#f0f3f7] disabled:opacity-50" onClick={(e) => { e.currentTarget.closest("details")?.removeAttribute("open"); void preview(); }}>查看待处理</button><button disabled={busy || working} className="w-full rounded-lg px-3 py-2 text-left text-sm hover:bg-[#f0f3f7] disabled:opacity-50" onClick={(e) => { e.currentTarget.closest("details")?.removeAttribute("open"); void rebuild(true); }}>完整重建</button></div></details>{busy ? <button className={secondaryClass} onClick={() => void invoke("semantic_cancel_index").then(() => setMessage("正在停止，等待当前请求结束")).catch((e) => setMessage(errorText(e)))}>停止更新</button> : <button disabled={working || !hasKey} className={primaryClass} onClick={() => void rebuild()}>更新索引</button>}</div>
          </section>
        </div>
        <div hidden={tab==="models" && provider!=="knowledge"}><KnowledgeSettings view={tab==="models" ? "config" : "index"} /></div>
      </div>
    </section>
  </div>;
}
