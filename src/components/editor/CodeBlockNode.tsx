import CodeBlock from "@tiptap/extension-code-block";
import { NodeViewContent, NodeViewWrapper, ReactNodeViewRenderer, type NodeViewProps } from "@tiptap/react";
import type { EditorView } from "@codemirror/view";
import { Code2, Eye } from "lucide-react";
import { useEffect, useRef, useState } from "react";

function CodeBlockView(props: NodeViewProps) {
  const mermaidBlock = props.node.attrs.language === "mermaid";
  const [source, setSource] = useState(!mermaidBlock);
  const [svg, setSvg] = useState("");
  const [error, setError] = useState("");
  const codeMirrorHost = useRef<HTMLDivElement>(null);
  const codeMirror = useRef<EditorView | null>(null);
  const propsRef = useRef(props);
  const syncing = useRef(false);
  propsRef.current = props;
  useEffect(() => {
    if (!mermaidBlock || source) return;
    let disposed = false;
    setError("");
    setSvg("");
    void (async () => {
      try {
        const { default: mermaid } = await import("mermaid");
        if (disposed) return;
        mermaid.initialize({ startOnLoad: false, securityLevel: "strict", theme: "neutral", suppressErrorRendering: true });
        const result = await mermaid.render(`qy-mermaid-${crypto.randomUUID()}`, props.node.textContent);
        if (!disposed) { setSvg(result.svg); setError(""); }
      } catch (cause) {
        if (!disposed) {
          setSvg("");
          setError(cause instanceof Error ? cause.message : String(cause));
        }
      }
    })();
    return () => { disposed = true; };
  }, [mermaidBlock, props.node.textContent, source]);
  useEffect(() => {
    if (!source || !codeMirrorHost.current) return;
    let disposed = false;
    void Promise.all([import("codemirror"), import("@codemirror/state"), import("@codemirror/view")]).then(([{ basicSetup }, { EditorState }, { EditorView }]) => {
      if (disposed || !codeMirrorHost.current) return;
      const view = new EditorView({
        parent: codeMirrorHost.current,
        state: EditorState.create({
          doc: propsRef.current.node.textContent,
          extensions: [basicSetup, EditorView.lineWrapping, EditorView.updateListener.of((update) => {
            if (!update.docChanged || syncing.current) return;
            const current = propsRef.current;
            const position = current.getPos();
            if (typeof position !== "number") return;
            const text = update.state.doc.toString();
            current.editor.view.dispatch(current.editor.state.tr.replaceWith(position + 1, position + current.node.nodeSize - 1, text ? current.editor.schema.text(text) : []));
          })],
        }),
      });
      codeMirror.current = view;
    }).catch((cause) => { if (!disposed) setError(cause instanceof Error ? cause.message : String(cause)); });
    return () => { disposed = true; codeMirror.current?.destroy(); codeMirror.current = null; };
  }, [source, props.editor]);
  useEffect(() => {
    const view = codeMirror.current;
    if (!view || view.state.doc.toString() === props.node.textContent) return;
    syncing.current = true;
    try { view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: props.node.textContent } }); }
    finally { syncing.current = false; }
  }, [props.node.textContent]);
  return <NodeViewWrapper className="qy-code-node">
    <div className="qy-code-node-toolbar" contentEditable={false}>
      <span>{mermaidBlock ? "Mermaid" : props.node.attrs.language || "代码"}</span>
      {mermaidBlock && <button type="button" onClick={() => setSource((value) => !value)}>{source ? <Eye size={14}/> : <Code2 size={14}/>}<span>{source ? "预览" : "源码"}</span></button>}
    </div>
    {source ? <div ref={codeMirrorHost} className="qy-code-mirror" contentEditable={false}/> : error ? <div className="qy-mermaid-error" contentEditable={false} role="alert"><strong>Mermaid 图表无法渲染</strong><pre>{error}</pre></div> : <div className="qy-mermaid-preview" contentEditable={false} dangerouslySetInnerHTML={{ __html: svg }}/>}
    <NodeViewContent className="hidden"/>
  </NodeViewWrapper>;
}

export const QingyuCodeBlock = CodeBlock.extend({
  addAttributes() {
    return { language: { default: null, parseHTML: (element) => element.firstElementChild?.className.match(/language-([\w-]+)/)?.[1] ?? null, renderHTML: (attributes) => attributes.language ? { class: `language-${attributes.language}` } : {} } };
  },
  addNodeView() { return ReactNodeViewRenderer(CodeBlockView); },
});
