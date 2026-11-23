import { useState } from "react";
import { FileDown, Loader2, Printer } from "lucide-react";
import { api, type SavedFile } from "../lib/api";
import { Button } from "./ui";
import { useToast } from "./Toast";

/** Print (open in the PDF viewer) and Save PDF buttons for a rendered document. */
export function PdfActions({ open, save, printTitle }: { open: () => Promise<unknown>; save: () => Promise<SavedFile | null>; printTitle?: string }) {
  const toast = useToast();
  const [busy, setBusy] = useState<null | "save" | "open">(null);
  const run = async (kind: "save" | "open") => {
    setBusy(kind);
    try {
      if (kind === "open") {
        await open();
      } else {
        const f = await save();
        if (f)
          toast(`Saved ${f.fileName}`, "success", {
            label: "Show in folder",
            onClick: () => void api.openSavedFile(f.path),
          });
      }
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), "error");
    } finally {
      setBusy(null);
    }
  };
  return (
    <>
      <Button disabled={!!busy} onClick={() => void run("open")} title={printTitle ?? "Open in your PDF viewer to print"}>
        {busy === "open" ? <Loader2 size={15} className="animate-spin" /> : <Printer size={15} />} Print
      </Button>
      <Button disabled={!!busy} onClick={() => void run("save")}>
        {busy === "save" ? <Loader2 size={15} className="animate-spin" /> : <FileDown size={15} />} Save PDF
      </Button>
    </>
  );
}
