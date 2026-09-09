import React, { Component, type ErrorInfo, type ReactNode } from "react";
import ReactDOM from "react-dom/client";
import App from "./app/App";
import "./styles/index.css";

class AppErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("轻羽界面渲染失败", error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return <main style={{ padding: 32, color: "#1d2939", fontFamily: '"Qingyu Source Han Sans", "Microsoft YaHei UI", sans-serif' }}>
        <h1 style={{ fontSize: 22, marginBottom: 12 }}>轻羽加载失败</h1>
        <p style={{ whiteSpace: "pre-wrap", color: "#b42318" }}>{this.state.error.message}</p>
      </main>;
    }
    return this.props.children;
  }
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <AppErrorBoundary><App /></AppErrorBoundary>
  </React.StrictMode>,
);
