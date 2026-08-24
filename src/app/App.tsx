import { WorkspaceShell } from "../components/layout/WorkspaceShell";
import { AgentPanel } from "../components/agent/AgentPanel";
import { MarkdownEditor } from "../components/editor/MarkdownEditor";
import { useState } from "react";

function App() {
  const [value, setValue] = useState("# 欢迎使用轻羽\n\n这是技术骨架预览。页面保存将在下一阶段接入。\n");
  return (
    <WorkspaceShell agentPanel={<AgentPanel />}>
      <div className="flex h-full flex-col">
        <div className="border-b border-[var(--qy-border)] px-8 py-6"><div className="text-[12px] text-[var(--qy-faint)]">我的笔记库</div><h1 className="mt-2 text-xl font-semibold">开始构建你的知识库</h1></div>
        <MarkdownEditor value={value} onChange={setValue} />
      </div>
    </WorkspaceShell>
  );
}

export default App;
