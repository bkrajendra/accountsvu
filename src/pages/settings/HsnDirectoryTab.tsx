import { useState, type ReactNode } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { CloudDownload, FileUp, Loader2 } from "lucide-react";
import { api, keys, type DirectoryDiff, type DirectorySyncProgress, type DirectorySyncResult } from "../../lib/api";
import { Badge, Button, Card } from "../../components/ui";
import { useToast } from "../../components/Toast";
import { HsnSearchPanel } from "../items/HsnDirectory";
import { SettingsSection as Section } from "./SettingsSection";

function when(iso: string | null): string {
  if (!iso) return "Never";
  return new Date(iso).toLocaleString([], { day: "numeric", month: "short", year: "numeric", hour: "numeric", minute: "2-digit" });
}

function diffText(d: DirectoryDiff): string {
  const parts = [
    d.added && `${d.added.toLocaleString()} added`,
    d.changed && `${d.changed.toLocaleString()} changed`,
    d.delisted && `${d.delisted.toLocaleString()} no longer listed`,
    d.restored && `${d.restored.toLocaleString()} listed again`,
  ].filter(Boolean);
  return parts.length ? parts.join(" · ") : "No changes";
}

function Row({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4 px-5 py-3">
      <dt className="text-muted">{label}</dt>
      <dd className="min-w-0 text-right font-medium">{value}</dd>
    </div>
  );
}

/** Settings › HSN/SAC directory (Phase 2.5.1): sync the GSTN master and search it. */
export function HsnDirectoryTab() {
  const qc = useQueryClient();
  const toast = useToast();
  const { data: status } = useQuery({ queryKey: keys.hsnDirectory, queryFn: api.hsnDirectoryStatus });
  const [progress, setProgress] = useState<DirectorySyncProgress | null>(null);

  function done(r: DirectorySyncResult | null) {
    setProgress(null);
    if (!r) return;
    qc.setQueryData(keys.hsnDirectory, r.status);
    void qc.invalidateQueries({ queryKey: ["hsnDirectorySearch"] });
    void qc.invalidateQueries({ queryKey: keys.audit });
    toast(r.unchanged ? "The HSN/SAC list hasn't changed since the last sync" : `HSN/SAC directory updated: ${diffText(r.diff)}`);
  }
  const failed = (e: Error) => {
    setProgress(null);
    toast(e.message, "error");
  };
  const sync = useMutation({ mutationFn: () => api.syncHsnDirectory(setProgress), onSuccess: done, onError: failed });
  const importFile = useMutation({ mutationFn: api.importHsnMasterFile, onSuccess: done, onError: failed });
  const busy = sync.isPending || importFile.isPending;

  const pct = progress?.total ? Math.min(100, Math.round((progress.received / progress.total) * 100)) : null;
  const syncLabel = !sync.isPending
    ? status?.ready
      ? "Sync again"
      : "Sync from GST portal"
    : progress?.stage === "updating"
      ? "Updating directory…"
      : pct !== null
        ? `Downloading ${pct}%`
        : "Downloading…";

  return (
    <>
      <Section
        title="HSN/SAC directory"
        description="A local copy of the HSN and SAC code list published on the GST portal, used to find codes when you add items. It's a lookup aid, shared by every business on this computer and not part of backups."
      >
        <Card>
          <dl className="divide-y divide-line text-[13.5px]">
            <Row
              label="Status"
              value={
                !status ? (
                  "—"
                ) : status.ready ? (
                  <Badge tone="success">
                    {status.hsnCodes.toLocaleString()} HSN · {status.sacCodes.toLocaleString()} SAC codes
                  </Badge>
                ) : (
                  <Badge tone="warning">Not synced yet</Badge>
                )
              }
            />
            <Row label="Last synced" value={when(status?.syncedAt ?? null)} />
            {status?.origin && (
              <Row
                label="Source"
                value={<span className="selectable break-all font-normal">{status.channel === "file" ? `Imported file ${status.origin}` : status.origin}</span>}
              />
            )}
            {status?.lastDiff && <Row label="Last change" value={<span className="font-normal">{diffText(status.lastDiff)}</span>} />}
            {status && status.delisted > 0 && (
              <Row label="No longer listed" value={<span className="font-normal">{status.delisted.toLocaleString()} codes, kept so items that use them still find them</span>} />
            )}
          </dl>
          <div className="flex flex-wrap items-center gap-2 border-t border-line px-5 py-4">
            <Button variant="primary" disabled={busy} onClick={() => sync.mutate()}>
              {sync.isPending ? <Loader2 size={15} className="animate-spin" /> : <CloudDownload size={15} />}
              {syncLabel}
            </Button>
            <Button disabled={busy} onClick={() => importFile.mutate()}>
              {importFile.isPending ? <Loader2 size={15} className="animate-spin" /> : <FileUp size={15} />}
              Import downloaded file…
            </Button>
            <p className="w-full pt-1 text-[12px] text-subtle">
              Downloads <span className="selectable">{status?.sourceUrl ?? "HSN_SAC.xlsx"}</span>. Without internet, download it elsewhere and import it. Codes the portal drops are kept and marked.
            </p>
          </div>
        </Card>
      </Section>

      <Section title="Search" description="Look up a code by its digits or by words in its description, heading or sub-heading.">
        <Card className="p-4">
          <HsnSearchPanel listClassName="max-h-[520px]" />
        </Card>
      </Section>
    </>
  );
}
