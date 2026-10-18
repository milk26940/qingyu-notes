import { LoaderCircle, X } from "lucide-react";
import { useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { getAgentSettings, saveAgentSettings, testAgentConnection, type AgentConnectionTest, type AgentSettings as Settings } from "../../lib/tauri/codex";
import { useCodexStore } from "../../stores/codexStore";
import { usePagesStore } from "../../stores/pagesStore";
import { useAgentStore } from "../../stores/agentStore";

export function AgentSettings({ close }: { close: () => void }) {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [baseUrl, setBaseUrl] = useState("");
  const [model, setModel] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [clearApiKey, setClearApiKey] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [result, setResult] = useState<AgentConnectionTest | null>(null);
  const loadModels = useCodexStore((state) => state.loadModels);
  const models = useCodexStore((state) => state.models);
  const modelsError = useCodexStore((state) => state.modelsError);
  const modelsLoading = useCodexStore((state) => state.modelsLoading);
  const connect = useCodexStore((state) => state.connect);
  const status = useCodexStore((state) => state.status);
  const vaultPath = usePagesStore((state) => state.vaultPath);
  useEffect(() => { let disposed = false; void getAgentSettings().then((value) => { if (disposed) return; setSettings(value); setBaseUrl(value.baseUrl); setModel(useAgentStore.getState().selectedModel ?? value.model); }).catch((failure) => { if (!disposed) setError(String(failure)); }); return () => { disposed = true; }; }, []);
  useEffect(() => { const escape = (event: KeyboardEvent) => { if (event.key === "Escape" && !busy) close(); }; window.addEventListener("keydown", escape); return () => window.removeEventListener("keydown", escape); }, [busy, close]);
  const save = async (test: boolean, refresh = false) => {
    setBusy(true); setError(""); setNotice(""); setResult(null);
    try {
      const value = await saveAgentSettings({ baseUrl: baseUrl.trim(), model: model.trim(), ...(apiKey.trim() ? { apiKey: apiKey.trim() } : {}), clearApiKey });
      setSettings(value); setApiKey(""); setClearApiKey(false); setBaseUrl(value.baseUrl); setModel(value.model);
      useAgentStore.getState().setSelectedModel(value.model);
      if (!test && !refresh) {
        void connect();
        close();
        return true;
      }
      await connect();
      if (refresh) {
        const refreshed = await loadModels();
        setNotice(refreshed ? `设置已保存，当前对话使用 ${value.model}，已获取 ${useCodexStore.getState().models.length} 个模型` : "设置已保存，模型列表未能刷新，请展开详细信息查看原因。");
      } else if (test) {
        const checked = await testAgentConnection(value.model); setResult(checked);
        setNotice(checked.text ? "设置已保存，连接可用" : "设置已保存，连接测试未通过");
      } else setNotice(`设置已保存，当前对话使用 ${value.model}`);
      return true;
    } catch (failure) { setError(typeof failure === "object" && failure && "message" in failure ? String(failure.message) : String(failure)); return false; }
    finally { setBusy(false); }
  };
  return createPortal(<div className="qy-change-overlay" onMouseDown={(event) => { if (event.target === event.currentTarget && !busy) close(); }}><section role="dialog" aria-modal="true" aria-label="内置 Agent 设置" className="qy-agent-settings">
    <header><strong>小羽设置</strong><button type="button" className="icon-button" aria-label="关闭 Agent 设置" disabled={busy} onClick={close}><X size={18}/></button></header>
    <p>配置兼容 OpenAI 的服务地址和模型，直接在轻羽中使用 Agent。</p>
    {!settings && !error && <p><LoaderCircle size={15} className="animate-spin"/>正在读取设置…</p>}
    <fieldset disabled={busy || !settings}>
      <label>服务地址<input autoFocus aria-label="Agent 服务地址" placeholder="https://api.openai.com/v1" value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)}/></label>
      <label>API Key<input type="password" autoComplete="new-password" aria-label="Agent API Key" placeholder={settings?.hasApiKey ? "已保存，留空保留原密钥" : "填写 API Key"} value={apiKey} onChange={(event) => { setApiKey(event.target.value); if (event.target.value) setClearApiKey(false); }}/></label>
      {settings?.hasApiKey && <label className="qy-agent-key-clear"><input type="checkbox" checked={clearApiKey} onChange={(event) => { setClearApiKey(event.target.checked); if (event.target.checked) setApiKey(""); }}/>清除已保存的密钥</label>}
      <label>当前对话模型<select aria-label="选择当前对话模型" value={model} disabled={modelsLoading} onChange={(event) => setModel(event.target.value)}>{!model && <option value="" disabled>{modelsLoading ? "正在获取模型…" : "请保存并刷新模型列表"}</option>}{model && !models.some((item) => item.model === model) && <option value={model}>{model}</option>}{models.map((item) => <option key={item.id} value={item.model}>{item.displayName}</option>)}</select><small>保存后用于当前对话的后续消息，并作为新对话的默认模型。</small></label>
      <button type="button" className="qy-agent-settings-refresh" onClick={() => void save(false, true)}>{busy ? <><LoaderCircle size={15} className="animate-spin"/>正在处理…</> : "保存并刷新模型"}</button>
      {modelsError && <small>模型列表暂不可用，请检查连接后保存并刷新。错误详情见下方。</small>}
    </fieldset>
    {error && <p role="alert" className="qy-agent-settings-error">操作未完成，请展开详细信息查看原因。</p>}
    <p role="status">连接状态：{result ? (result.text ? "可用" : "测试未通过") : status.state === "ready" ? "已配置，等待连接测试" : "尚未就绪"}{!result && " · 文字／工具／图片能力待测试"}</p>
    {notice && <p role="status">{notice}</p>}
    {result && <div className="qy-agent-capabilities"><strong>{result.model}</strong><p>文本：{result.text ? "可用" : "不可用"} · 工具：{result.tools ? "可用" : "不可用"} · 图片：{result.images ? "可用" : "不可用"}</p></div>}
    <details><summary>详细信息</summary><p>只允许访问当前笔记库中的页面、数据库和已导入附件。</p>{vaultPath && <label>当前笔记库<output>{vaultPath}</output></label>}{settings?.workspaceRoot && <label>运行目录<output>{settings.workspaceRoot}</output></label>}{(error || modelsError || result?.details.length) && <><pre>{[error, modelsError, ...(result?.details ?? [])].filter(Boolean).join("\n")}</pre><button type="button" onClick={() => void navigator.clipboard.writeText([error, modelsError, ...(result?.details ?? [])].filter(Boolean).join("\n")).then(() => setNotice("详情已复制")).catch(() => setNotice("复制失败，请手动选择详情复制"))}>复制详情</button></>}</details>
    <footer><button type="button" disabled={busy || !settings || !baseUrl.trim() || !model.trim()} onClick={() => void save(true)}>{busy ? "正在处理…" : "保存并测试连接"}</button><button type="button" disabled={busy || !settings || !baseUrl.trim() || !model.trim()} onClick={() => void save(false)}>保存设置</button></footer>
  </section></div>, document.body);
}
