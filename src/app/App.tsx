import { WorkspaceShell } from "../components/layout/WorkspaceShell";
import { AgentPanel } from "../components/agent/AgentPanel";

function App() {
  return (
    <WorkspaceShell agentPanel={<AgentPanel />}>
      <div className="flex h-full flex-col">
        <div className="border-b border-[var(--qy-border)] px-8 py-6"><div className="text-[12px] text-[var(--qy-faint)]">我的笔记库</div><h1 className="mt-2 text-xl font-semibold">开始构建你的知识库</h1></div>
        <div className="flex flex-1 items-center justify-center text-[13px] text-[var(--qy-muted)]">选择一个页面，或从左侧创建新页面</div>
      </div>
    </WorkspaceShell>
  );
}

export default App;
