import { ArrowDownAZ, Filter, Plus, X } from "lucide-react";
import { useEffect, useState } from "react";
import type { DatabaseFieldType, DatabaseFilter, DatabaseSort, FieldDefinition, FilterOperator } from "../../lib/tauri/databases";
import type { Page } from "../../lib/tauri/pages";
import { useDatabaseStore } from "../../stores/databaseStore";
import { usePagesStore } from "../../stores/pagesStore";

const fieldLabels: Record<DatabaseFieldType, string> = { text: "文本", number: "数字", select: "单选", multiSelect: "多选", date: "日期", checkbox: "勾选", relation: "关系" };
const operatorLabels: Record<FilterOperator, string> = { equals: "等于", notEquals: "不等于", contains: "包含", greaterThan: "大于", lessThan: "小于", isEmpty: "为空", isNotEmpty: "不为空" };

function PropertyEditor({ field, record }: { field: FieldDefinition; record: Page }) {
  const updateRecord = useDatabaseStore((state) => state.updateRecord);
  const pages = usePagesStore((state) => state.pages);
  const value = record.properties?.[field.id];
  const commit = (next: unknown) => void updateRecord(record, record.title, { ...(record.properties ?? {}), [field.id]: next });
  if (field.fieldType === "checkbox") return <input type="checkbox" checked={Boolean(value)} onChange={(event) => commit(event.target.checked)} aria-label={`${record.title}-${field.name}`} />;
  if (field.fieldType === "select") return <select value={String(value ?? "")} onChange={(event) => commit(event.target.value)} className="w-full bg-transparent text-[12px]"><option value="" />{field.options.map((option) => <option key={option}>{option}</option>)}</select>;
  if (field.fieldType === "multiSelect") return <select multiple value={Array.isArray(value) ? value.map(String) : []} onChange={(event) => commit(Array.from(event.currentTarget.selectedOptions, (option) => option.value))} className="h-14 w-full bg-transparent text-[11px]">{field.options.map((option) => <option key={option} value={option}>{option}</option>)}</select>;
  if (field.fieldType === "relation") return <select multiple value={Array.isArray(value) ? value.map(String) : []} onChange={(event) => commit(Array.from(event.currentTarget.selectedOptions, (option) => option.value))} className="h-14 w-full bg-transparent text-[11px]" aria-label={`${record.title}-${field.name}`}>{pages.filter((page) => page.id !== record.id).map((page) => <option key={page.id} value={page.id}>{page.title}</option>)}</select>;
  return <input type={field.fieldType === "number" ? "number" : field.fieldType === "date" ? "date" : "text"} defaultValue={value == null ? "" : String(value)} onBlur={(event) => commit(field.fieldType === "number" ? (event.target.value ? Number(event.target.value) : null) : event.target.value)} className="w-full bg-transparent outline-none" />;
}

function FilterControl({ fields, filters, onChange }: { fields: FieldDefinition[]; filters: DatabaseFilter[]; onChange: (filters: DatabaseFilter[]) => Promise<void> }) {
  const [open, setOpen] = useState(false);
  const [fieldId, setFieldId] = useState("");
  const [operator, setOperator] = useState<FilterOperator>("equals");
  const [value, setValue] = useState("");
  const add = () => { if (!fieldId) return; const field = fields.find((item) => item.id === fieldId); const parsed = field?.fieldType === "number" ? Number(value) : field?.fieldType === "checkbox" ? value === "true" : value; void onChange([...filters, { fieldId, operator, value: operator === "isEmpty" || operator === "isNotEmpty" ? null : parsed }]); setOpen(false); };
  return <div className="relative"><button className="inline-flex h-7 items-center gap-1 rounded-md px-2 text-[12px] text-[var(--qy-muted)] hover:bg-[var(--qy-bg)]" onClick={() => setOpen((current) => !current)}><Filter size={14} />筛选{filters.length ? ` ${filters.length}` : ""}</button>{open && <div className="absolute right-0 top-8 z-20 flex w-80 gap-1 border border-[var(--qy-border)] bg-white p-2 shadow-lg"><select value={fieldId} onChange={(event) => setFieldId(event.target.value)} className="min-w-0 flex-1 border border-[var(--qy-border)] px-1"><option value="">字段</option>{fields.map((field) => <option key={field.id} value={field.id}>{field.name}</option>)}</select><select value={operator} onChange={(event) => setOperator(event.target.value as FilterOperator)} className="border border-[var(--qy-border)] px-1">{Object.entries(operatorLabels).map(([key, label]) => <option key={key} value={key}>{label}</option>)}</select>{!operator.startsWith("is") && <input value={value} onChange={(event) => setValue(event.target.value)} className="w-20 border border-[var(--qy-border)] px-1" aria-label="筛选值" />}<button className="icon-button" onClick={add} aria-label="添加筛选"><Plus size={14}/></button></div>} {filters.length > 0 && <button className="icon-button h-7 w-7" onClick={() => void onChange([])} aria-label="清除筛选"><X size={13}/></button>}</div>;
}

function SortControl({ fields, sorts, onChange }: { fields: FieldDefinition[]; sorts: DatabaseSort[]; onChange: (sorts: DatabaseSort[]) => Promise<void> }) {
  const [open, setOpen] = useState(false);
  const [fieldId, setFieldId] = useState("");
  const [direction, setDirection] = useState<DatabaseSort["direction"]>("ascending");
  return <div className="relative"><button className="inline-flex h-7 items-center gap-1 rounded-md px-2 text-[12px] text-[var(--qy-muted)] hover:bg-[var(--qy-bg)]" onClick={() => setOpen((current) => !current)}><ArrowDownAZ size={14} />排序{sorts.length ? ` ${sorts.length}` : ""}</button>{open && <div className="absolute right-0 top-8 z-20 flex w-60 gap-1 border border-[var(--qy-border)] bg-white p-2 shadow-lg"><select value={fieldId} onChange={(event) => setFieldId(event.target.value)} className="min-w-0 flex-1 border border-[var(--qy-border)] px-1"><option value="">字段</option>{fields.map((field) => <option key={field.id} value={field.id}>{field.name}</option>)}</select><select value={direction} onChange={(event) => setDirection(event.target.value as DatabaseSort["direction"])} className="border border-[var(--qy-border)] px-1"><option value="ascending">升序</option><option value="descending">降序</option></select><button className="icon-button" aria-label="添加排序" onClick={() => { if (fieldId) void onChange([...sorts.filter((sort) => sort.fieldId !== fieldId), { fieldId, direction }]); setOpen(false); }}><Plus size={14}/></button></div>}{sorts.length > 0 && <button className="icon-button h-7 w-7" onClick={() => void onChange([])} aria-label="清除排序"><X size={13}/></button>}</div>;
}

export function DatabaseTable({ databaseId }: { databaseId: string }) {
  const { bundle, records, filters, sorts, open, addField, addRecord, updateRecord, setFilters, setSorts } = useDatabaseStore();
  const openPage = usePagesStore((state) => state.open);
  const [fieldName, setFieldName] = useState("");
  const [fieldType, setFieldType] = useState<DatabaseFieldType>("text");
  const [fieldOptions, setFieldOptions] = useState("");
  useEffect(() => { void open(databaseId); }, [databaseId, open]);
  if (!bundle || bundle.page.id !== databaseId) return <div className="grid h-full place-items-center text-sm text-[var(--qy-muted)]">正在加载数据库</div>;
  const fields = bundle.definition.fields;
  const createField = () => { if (!fieldName.trim()) return; const options = fieldOptions.split(",").map((option) => option.trim()).filter(Boolean); void addField(fieldName.trim(), fieldType, options); setFieldName(""); setFieldOptions(""); };
  return <div className="flex h-full min-h-0 flex-col">
    <div className="flex items-center justify-between border-b border-[var(--qy-border)] px-6 py-5"><div><div className="text-[11px] text-[var(--qy-faint)]">数据库 · 表格视图</div><h1 className="mt-1 text-xl font-semibold">{bundle.page.title}</h1></div><div className="flex items-center gap-1"><FilterControl fields={fields} filters={filters} onChange={setFilters} /><SortControl fields={fields} sorts={sorts} onChange={setSorts} /><button className="ml-2 inline-flex h-7 items-center gap-1 rounded-md bg-[var(--qy-accent)] px-2 text-[12px] text-white" onClick={() => void addRecord()}><Plus size={14} />新建记录</button></div></div>
    <div className="min-h-0 flex-1 overflow-auto px-6 py-4"><table className="w-full border-collapse text-left text-[12px]"><thead><tr><th className="min-w-48 border border-[var(--qy-border)] bg-[var(--qy-bg)] px-2 py-2 font-medium">标题</th>{fields.map((field) => <th key={field.id} className="min-w-36 border border-[var(--qy-border)] bg-[var(--qy-bg)] px-2 py-2 font-medium">{field.name}<span className="ml-2 font-normal text-[var(--qy-faint)]">{fieldLabels[field.fieldType]}</span></th>)}<th className="min-w-72 border border-[var(--qy-border)] bg-[var(--qy-bg)] px-2 py-1"><div className="flex gap-1"><input aria-label="字段名称" value={fieldName} onChange={(event) => setFieldName(event.target.value)} placeholder="新字段" className="min-w-0 flex-1 bg-transparent outline-none" /><select value={fieldType} onChange={(event) => setFieldType(event.target.value as DatabaseFieldType)} className="w-16 bg-transparent">{Object.entries(fieldLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select>{(fieldType === "select" || fieldType === "multiSelect") && <input aria-label="字段选项" value={fieldOptions} onChange={(event) => setFieldOptions(event.target.value)} placeholder="选项，逗号分隔" className="w-28 bg-transparent outline-none" />}<button aria-label="添加字段" onClick={createField}><Plus size={14}/></button></div></th></tr></thead><tbody>{records.map((record) => <tr key={record.id}><td className="border border-[var(--qy-border)] px-2 py-2"><input defaultValue={record.title} onBlur={(event) => void updateRecord(record, event.target.value, record.properties ?? {})} onDoubleClick={() => void openPage(record.id)} className="w-full bg-transparent font-medium outline-none" /></td>{fields.map((field) => <td key={field.id} className="border border-[var(--qy-border)] px-2 py-2"><PropertyEditor field={field} record={record} /></td>)}<td className="border border-[var(--qy-border)] px-2 py-2 text-[var(--qy-faint)]">双击标题打开页面</td></tr>)}</tbody></table>{records.length === 0 && <div className="border-x border-b border-[var(--qy-border)] py-8 text-center text-[12px] text-[var(--qy-faint)]">暂无记录</div>}</div>
  </div>;
}
