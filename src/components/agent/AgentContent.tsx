import { memo, useEffect, useMemo, useState } from "react";
import { FileText, X } from "lucide-react";
import DOMPurify from "dompurify";
import { marked } from "marked";
import { usePagesStore } from "../../stores/pagesStore";
import { parseQingyuPageLink } from "../../lib/internalLinks";
import { readConversationAttachment, type ConversationAttachment } from "../../lib/tauri/conversations";

export const AgentMarkdown = memo(function AgentMarkdown({ text }: { text: string }) {
  const openPage = usePagesStore((state) => state.open);
  const html = useMemo(
    () => DOMPurify.sanitize(marked.parse(text, { gfm: true, breaks: true }) as string, {
      ALLOWED_URI_REGEXP: /^(?:(?:https?|mailto|qingyu):|[^a-z]|[a-z+.-]+(?:[^a-z+.-:]|$))/i,
    }),
    [text],
  );
  return <div className="qy-agent-markdown" onClick={(event) => {
    const anchor = (event.target as HTMLElement).closest("a");
    if (!anchor) return;
    const target = parseQingyuPageLink(anchor.getAttribute("href") ?? "");
    if (!target) return;
    event.preventDefault();
    void openPage(target.pageId, target.blockId);
  }} dangerouslySetInnerHTML={{ __html: html }} />;
});

export function AttachmentPreview({
  attachment,
  removable,
  onRemove,
}: {
  attachment: ConversationAttachment;
  removable?: boolean;
  onRemove?: () => void;
}) {
  const [source, setSource] = useState("");
  useEffect(() => {
    if (!attachment.mime.startsWith("image/")) return;
    let active = true;
    let url = "";
    void readConversationAttachment(attachment.relativePath, attachment.mime)
      .then((data) => {
        if (!active) return;
        url = URL.createObjectURL(
          new Blob([new Uint8Array(data.bytes)], { type: data.mime }),
        );
        setSource(url);
      })
      .catch(() => undefined);
    return () => {
      active = false;
      if (url) URL.revokeObjectURL(url);
    };
  }, [attachment.mime, attachment.relativePath]);
  return (
    <div className="relative flex max-w-full items-center gap-1 rounded border border-[var(--qy-border)] bg-white p-1 text-[12px]">
      {source ? (
        <img
          src={source}
          alt={attachment.name}
          className="h-14 w-14 rounded object-cover"
        />
      ) : (
        <FileText size={14} />
      )}
      <span className="max-w-36 truncate px-1">{attachment.name}</span>
      {removable && (
        <button
          type="button"
          className="self-start"
          aria-label={`移除附件 ${attachment.name}`}
          onClick={onRemove}
        >
          <X size={12} />
        </button>
      )}
    </div>
  );
}
