import avatar from "../../assets/agent/xiaoyu-avatar.webp";

export function AgentAvatar({ className = "" }: { className?: string }) {
  return <img className={`qy-agent-avatar ${className}`} src={avatar} alt="" aria-hidden="true" draggable={false}/>;
}
