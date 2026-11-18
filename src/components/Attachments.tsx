import { useRef, useState, type DragEvent } from "react";
import { useQueryClient } from "@tanstack/react-query";
import clsx from "clsx";
import { FileText, Image as ImageIcon, Loader2, Paperclip, X } from "lucide-react";
import { api, ApiError, keys, type Attachment } from "../lib/api";
import { fileSize } from "../lib/purchases";
import { useToast } from "./Toast";
import { Card } from "./ui";

/**
 * Original bills and receipts (spec §5.3). Each file is stored with its
 * SHA-256 so it can be shown to be unaltered later. Files can be removed only
 * while the document is a draft.
 */
export function AttachmentsPanel({ documentId, attachments, editable }: { documentId: string; attachments: Attachment[]; editable: boolean }) {
  const qc = useQueryClient();
  const toast = useToast();
  const input = useRef<HTMLInputElement>(null);
  const [busy, setBusy] = useState(false);
  const [over, setOver] = useState(false);

  const refresh = () => void qc.invalidateQueries({ queryKey: keys.purchase(documentId) });

  async function upload(files: FileList | null) {
    if (!files?.length) return;
    setBusy(true);
    try {
      for (const f of Array.from(files)) {
        await api.addAttachment(documentId, f.name, new Uint8Array(await f.arrayBuffer()));
      }
      toast(files.length === 1 ? "File attached" : `${files.length} files attached`);
      refresh();
    } catch (e) {
      toast(e instanceof ApiError ? e.message : String(e), "error");
    } finally {
      setBusy(false);
    }
  }

  async function open(a: Attachment) {
    try {
      const c = await api.attachmentContent(a.id);
      const url = URL.createObjectURL(new Blob([new Uint8Array(c.bytes)], { type: c.contentType }));
      const link = Object.assign(document.createElement("a"), { href: url, target: "_blank", rel: "noopener" });
      if (!c.contentType.startsWith("image/") && c.contentType !== "application/pdf") link.download = c.fileName;
      link.click();
      setTimeout(() => URL.revokeObjectURL(url), 60_000);
    } catch (e) {
      toast(e instanceof ApiError ? e.message : String(e), "error");
    }
  }

  async function remove(a: Attachment) {
    try {
      await api.removeAttachment(a.id);
      toast("Attachment removed");
      refresh();
    } catch (e) {
      toast(e instanceof ApiError ? e.message : String(e), "error");
    }
  }

  const drop = (e: DragEvent) => {
    e.preventDefault();
    setOver(false);
    void upload(e.dataTransfer.files);
  };

  return (
    <Card className="p-4 text-[13px]">
      <div className="mb-2 flex items-center justify-between">
        <span className="text-[12px] font-semibold uppercase tracking-[0.04em] text-muted">Attachments</span>
        {busy && <Loader2 size={14} className="animate-spin text-muted" />}
      </div>
      {attachments.length > 0 && (
        <ul className="mb-3 space-y-1">
          {attachments.map((a) => (
            <li key={a.id} className="group flex items-center gap-2 rounded-lg px-1.5 py-1 hover:bg-hover">
              {a.contentType.startsWith("image/") ? <ImageIcon size={15} className="shrink-0 text-muted" /> : <FileText size={15} className="shrink-0 text-muted" />}
              <button type="button" onClick={() => void open(a)} className="min-w-0 flex-1 text-left" title={`SHA-256 ${a.sha256}`}>
                <div className="truncate">{a.fileName}</div>
                <div className="num text-[11.5px] text-subtle">
                  {fileSize(a.sizeBytes)} · {a.sha256.slice(0, 12)}…
                </div>
              </button>
              {editable && (
                <button
                  type="button"
                  aria-label={`Remove ${a.fileName}`}
                  onClick={() => void remove(a)}
                  className="grid size-6 place-items-center rounded-md text-subtle opacity-0 hover:text-fg group-hover:opacity-100 focus-visible:opacity-100"
                >
                  <X size={13} />
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
      <button
        type="button"
        disabled={busy}
        onClick={() => input.current?.click()}
        onDragOver={(e) => {
          e.preventDefault();
          setOver(true);
        }}
        onDragLeave={() => setOver(false)}
        onDrop={drop}
        className={clsx(
          "pressable flex w-full items-center justify-center gap-2 rounded-xl border border-dashed px-3 py-3 text-muted",
          over ? "border-accent-fill bg-accent-soft text-accent" : "border-line-strong hover:bg-surface-2",
        )}
      >
        <Paperclip size={14} /> Attach the supplier's bill
      </button>
      <input ref={input} type="file" hidden multiple accept=".pdf,.png,.jpg,.jpeg,.webp,.heic,.xlsx,.csv" onChange={(e) => {
        void upload(e.target.files);
        e.target.value = "";
      }} />
      <p className="mt-2 text-[11.5px] text-subtle">PDF or image, up to 5 MB. Kept with a SHA-256 fingerprint; can't be removed once recorded.</p>
    </Card>
  );
}
