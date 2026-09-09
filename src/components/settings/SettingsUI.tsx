import type { ReactNode } from "react";
export const inputClass = "mt-2 h-11 w-full rounded-xl border border-[#d9dde3] bg-white px-3 text-[14px] font-medium text-[#252a32] outline-none transition-colors focus:border-[#7b9fdd]";
export const secondaryClass = "rounded-xl border border-[#d9dde3] bg-white px-3.5 py-2 text-sm font-semibold text-[#343b46] transition-colors duration-200 hover:bg-[#f1f3f6] disabled:cursor-not-allowed disabled:opacity-50";
export const primaryClass = "rounded-xl bg-[#2463eb] px-4 py-2 text-sm font-semibold text-white transition-colors duration-200 hover:bg-[#1d54ce] disabled:cursor-not-allowed disabled:opacity-50";
export function SettingField({ label, children }: { label: string; children: ReactNode }) {
  return <label className="block text-sm font-semibold text-[#343b46]">{label}{children}</label>;
}
export function SettingNotice({ text }: { text: string }) {
  if (!text) return null;
  const [summary, ...details] = text.split("\n");
  return <div role="status" className="mt-4 rounded-xl bg-[#f0f3f7] px-4 py-3 text-sm leading-6 text-[#3c4552]"><p>{summary}</p>{details.some(Boolean) && <details className="mt-1"><summary className="cursor-pointer font-semibold">查看详情</summary><div className="mt-2 max-h-40 overflow-auto whitespace-pre-wrap break-words">{details.join("\n")}</div></details>}</div>;
}
export function IndexProgress({ done, total, title }: { done: number; total: number; title: string }) {
  return <div className="mt-4"><div className="mb-2 flex items-center justify-between gap-3 text-sm text-[#4b5563]"><span className="truncate">{title || "正在准备"}</span><span className="shrink-0 tabular-nums">{Math.min(done, total)} / {total}</span></div><div role="progressbar" aria-label="更新进度" aria-valuemin={0} aria-valuemax={Math.max(total, 1)} aria-valuenow={Math.min(done, total)} className="h-1.5 overflow-hidden rounded-full bg-[#e6eaf0]"><div className="h-full rounded-full bg-[#2463eb] transition-[width] duration-200" style={{ width: `${Math.min(100, Math.max(0, done / Math.max(total, 1) * 100))}%` }} /></div></div>;
}
