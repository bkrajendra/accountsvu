import { useRef, useState, type ReactNode } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "react-router";
import clsx from "clsx";
import { ArrowLeft, Download, FileJson, Loader2, Upload } from "lucide-react";
import {
  api,
  keys,
  runningInTauri,
  type Gstr2b,
  type ReconAmounts,
  type ReconDecision,
  type ReconRow,
  type ReconStatus,
} from "../../lib/api";
import { formatPaise } from "../../lib/format";
import { shortDate } from "../../lib/sales";
import { Badge, Button, Card, Segmented } from "../../components/ui";
import { useToast } from "../../components/Toast";
import { Amount, Loading } from "../reports/common";
import { useReturnScope } from "./ReturnCentre";
import { ReturnSwitch } from "./Gstr1Page";

const STATUS: Record<ReconStatus, { label: string; tone: "success" | "accent" | "warning" | "danger" | "neutral"; help: string }> = {
  exact: { label: "Exact match", tone: "success", help: "Same supplier, number, date and amounts." },
  likely: { label: "Likely match", tone: "accent", help: "Same supplier and amounts; the number differs. Accept if it's the same document." },
  mismatch: { label: "Mismatch", tone: "warning", help: "Same document number, but the date or an amount differs." },
  missing_in_2b: { label: "Missing in 2B", tone: "warning", help: "Recorded in your books but not in this GSTR-2B. Defer to wait for the supplier to file; it shows again next month." },
  missing_in_books: { label: "Missing in books", tone: "danger", help: "In GSTR-2B but not recorded as a purchase." },
  duplicate: { label: "Duplicate in 2B", tone: "neutral", help: "The same document appears twice in GSTR-2B." },
};

const FIELD: Record<string, string> = {
  number: "number",
  date: "date",
  taxable: "taxable value",
  igst: "IGST",
  cgst: "CGST",
  sgst: "SGST",
  cess: "cess",
};

type Filter = "review" | "all" | ReconStatus;

export function Gstr2bPage() {
  const { period = "" } = useParams();
  const scope = useReturnScope();
  const { data: centre } = useQuery({
    queryKey: keys.report("gst", "centre", "", ""),
    queryFn: () => api.returnCentre(null, null),
    enabled: !scope.reg,
  });
  const reg = scope.reg ?? centre?.registrationId ?? null;
  const qkey = keys.report("gst", "gstr2b", reg ?? "", period);
  const { data, error } = useQuery({
    queryKey: qkey,
    queryFn: () => api.gstr2b(reg!, period),
    enabled: !!reg,
  });
  const [filter, setFilter] = useState<Filter | null>(null);

  if (error) return <Card className="px-4 py-10 text-center text-[13px] text-danger">{error instanceof Error ? error.message : String(error)}</Card>;
  if (!data) return <Loading />;

  const active: Filter = filter ?? (data.needsReview > 0 ? "review" : "all");
  const rows = data.rows.filter((r) => (active === "all" ? true : active === "review" ? r.needsReview : r.status === active));
  const options = [
    { value: "review" as Filter, label: "To review", count: data.needsReview },
    { value: "all" as Filter, label: "All", count: data.rows.length },
    ...data.summary.filter((s) => s.count > 0).map((s) => ({ value: s.status as Filter, label: STATUS[s.status].label, count: s.count })),
  ];

  return (
    <div>
      <Link to={`/gst?reg=${data.registrationId}`} className="mb-2 inline-flex items-center gap-1 text-[13px] text-muted hover:text-fg">
        <ArrowLeft size={14} /> GST returns
      </Link>
      <div className="mb-5 flex flex-wrap items-end justify-between gap-4">
        <div>
          <ReturnSwitch current="gstr2b" period={data.period} reg={data.registrationId} />
          <h1 className="text-display">{data.label}</h1>
          <p className="mt-1 text-[14px] text-muted">
            <span className="num text-fg">{data.gstin}</span> ·{" "}
            {data.statement
              ? `${data.statement.documentCount} documents in 2B${data.statement.generatedOn ? `, generated ${shortDate(data.statement.generatedOn)}` : ""} · imported ${shortDate(data.statement.importedAt.slice(0, 10))}`
              : "GSTR-2B not imported yet"}
          </p>
        </div>
        <div className="flex shrink-0 items-start gap-2">
          <ImportButton data={data} primary={!data.statement} />
          {data.statement && <ExportButton data={data} />}
        </div>
      </div>

      {!data.statement && (
        <Card className="mb-5 flex items-center gap-4 px-5 py-4">
          <div className="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">
            <FileJson size={20} />
          </div>
          <div className="text-[13.5px]">
            <div className="font-medium">Import GSTR-2B to reconcile {data.label}</div>
            <div className="text-muted">
              On the GST portal, open Returns → GSTR-2B for {data.label}, download the JSON file (unzip it if it comes as a ZIP), then import it here. Until then every purchase
              below shows as missing in 2B.
            </div>
          </div>
        </Card>
      )}

      {data.statement?.skippedSummary && (
        <Card className="mb-5 px-4 py-2.5 text-[13px] text-muted">
          Also in this 2B but not reconciled here: {data.statement.skippedSummary} (ISD credit and imports). Review them on the portal.
        </Card>
      )}

      <div className="mb-5 grid grid-cols-2 gap-4 lg:grid-cols-4">
        <Stat label="To review" text={String(data.needsReview)} note={data.needsReview ? "rows waiting for accept or defer" : "nothing left to review"} strong={data.needsReview > 0} />
        <Stat label="ITC available in 2B" text={formatPaise(data.itc.portalAvailablePaise)} note={data.itc.portalNotAvailablePaise ? `${formatPaise(data.itc.portalNotAvailablePaise)} marked not available` : "documents marked ITC available"} />
        <Stat label="ITC claimable in books" text={formatPaise(data.itc.booksClaimablePaise)} note="after each line's ITC review" />
        <Stat
          label="Claimed, not in 2B"
          text={formatPaise(data.itc.booksClaimedNotIn2bPaise)}
          note={data.itc.booksClaimedNotIn2bPaise ? "check before claiming in GSTR-3B" : "every claim is backed by 2B"}
          warn={data.itc.booksClaimedNotIn2bPaise > 0}
        />
      </div>

      <Card className="overflow-hidden">
        <div className="flex flex-wrap items-center justify-between gap-3 border-b border-line px-4 py-2">
          <h2 className="text-[13.5px] font-semibold">Purchases and GSTR-2B</h2>
          <Segmented<Filter> label="Show" value={active} onChange={setFilter} options={options} />
        </div>
        <div className="overflow-x-auto">
          <table className="w-full min-w-[920px] text-[13px]">
            <thead>
              <tr className="border-b border-line bg-surface-2 text-left text-[12px] text-muted">
                <Th>Supplier</Th>
                <Th>In books</Th>
                <Th>In GSTR-2B</Th>
                <Th right>Taxable value</Th>
                <Th right>GST</Th>
                <Th>Result</Th>
                <Th right>Decision</Th>
              </tr>
            </thead>
            <tbody className="divide-y divide-line">
              {rows.map((r) => (
                <Row key={r.key} row={r} data={data} qkey={qkey} />
              ))}
              {rows.length === 0 && (
                <tr>
                  <td colSpan={7} className="px-4 py-8 text-center text-muted">
                    {active === "review" ? "Nothing left to review." : "No documents."}
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </div>
      </Card>
      <p className="mt-3 text-[12px] text-muted">
        Matching compares supplier GSTIN, document number (ignoring case, punctuation and leading zeros), date and amounts, treating differences up to{" "}
        {formatPaise(data.tolerancePaise)} as equal. A match never claims credit: input tax credit follows the review on each purchase line.
      </p>
    </div>
  );
}

function Th({ children, right }: { children?: ReactNode; right?: boolean }) {
  return <th className={clsx("px-3 py-2 font-medium first:pl-4 last:pr-4", right && "text-right")}>{children}</th>;
}

function Stat({ label, text, note, strong, warn }: { label: string; text: string; note: string; strong?: boolean; warn?: boolean }) {
  return (
    <Card className="px-4 py-3">
      <div className="text-[12px] font-medium text-muted">{label}</div>
      <div className={clsx("num mt-1 text-[22px] font-semibold tracking-tight", strong && "text-accent", warn && "text-warning")}>{text}</div>
      <div className="mt-0.5 text-[12px] text-muted">{note}</div>
    </Card>
  );
}

const tax = (a: ReconAmounts) => a.taxPaise;

function Pair({ books, portal, differs }: { books?: number; portal?: number; differs: boolean }) {
  return (
    <div className="flex flex-col items-end leading-tight">
      {books !== undefined && <Amount paise={books} zero />}
      {portal !== undefined && (books === undefined || differs) && (
        <span className={clsx("text-[12px]", books !== undefined && "text-warning")}>
          {books !== undefined && "2B "}
          <Amount paise={portal} zero />
        </span>
      )}
    </div>
  );
}

function Row({ row: r, data, qkey }: { row: ReconRow; data: Gstr2b; qkey: readonly unknown[] }) {
  const navigate = useNavigate();
  const diff = new Set(r.differences.map((d) => d.field));
  const taxDiffers = ["igst", "cgst", "sgst", "cess"].some((f) => diff.has(f as never));
  const s = STATUS[r.status];
  return (
    <tr className={clsx("align-top", r.foundIn && "opacity-60")}>
      <td className="px-4 py-2.5">
        <div className="font-medium">{r.supplierName || "—"}</div>
        <div className="num text-[12px] text-muted">{r.supplierGstin}</div>
      </td>
      <td className="px-3 py-2.5">
        {r.books ? (
          <button type="button" onClick={() => navigate(`/purchases/${r.books!.purchaseId}`)} className="text-left">
            <span className="num text-accent hover:underline">{r.books.number}</span>
            <div className="text-[12px] text-muted">
              {shortDate(r.books.date)}
              {r.kind !== "invoice" && " · debit note"}
              {r.books.carried && " · earlier month"}
            </div>
          </button>
        ) : (
          <span className="text-subtle">—</span>
        )}
      </td>
      <td className="px-3 py-2.5">
        {r.portal ? (
          <>
            <span className={clsx("num", diff.has("number") && "text-warning")}>{r.portal.number}</span>
            <div className={clsx("text-[12px] text-muted", diff.has("date") && "text-warning")}>
              {shortDate(r.portal.date)}
              {r.kind !== "invoice" && ` · ${r.kind === "credit_note" ? "credit" : "debit"} note`}
              {r.portal.section.endsWith("a") && " · amended"}
            </div>
          </>
        ) : (
          <span className="text-subtle">—</span>
        )}
      </td>
      <td className="px-3 py-2.5 text-right">
        <Pair books={r.books?.amounts.taxablePaise} portal={r.portal?.amounts.taxablePaise} differs={diff.has("taxable")} />
      </td>
      <td className="px-3 py-2.5 text-right">
        <Pair books={r.books ? tax(r.books.amounts) : undefined} portal={r.portal ? tax(r.portal.amounts) : undefined} differs={taxDiffers} />
      </td>
      <td className="px-3 py-2.5">
        <div className="flex flex-wrap items-center gap-1" title={s.help}>
          {r.foundIn ? <Badge>In 2B for {r.foundIn}</Badge> : <Badge tone={s.tone}>{s.label}</Badge>}
          {r.portal && !r.portal.itcAvailable && <Badge tone="danger">ITC not available{r.portal.itcReason ? ` (${r.portal.itcReason})` : ""}</Badge>}
          {r.portal?.reverseCharge && <Badge>RCM</Badge>}
          {r.books && r.books.itcPendingLines > 0 && <Badge tone="warning">ITC review pending</Badge>}
        </div>
        {r.differences.length > 0 && (
          <div className="mt-1 text-[12px] text-muted">
            {r.differences
              .filter((d) => d.field !== "number" || r.status !== "likely")
              .map((d) => (d.deltaPaise != null ? `${FIELD[d.field]} ${d.deltaPaise > 0 ? "+" : ""}${formatPaise(d.deltaPaise)} in 2B` : `${FIELD[d.field]} differs`))
              .join(" · ")}
          </div>
        )}
        {r.note && <div className="mt-1 text-[12px] italic text-muted">{r.note}</div>}
      </td>
      <td className="px-4 py-2.5 text-right">
        {!r.foundIn && r.status !== "exact" && <DecisionCell row={r} data={data} qkey={qkey} />}
      </td>
    </tr>
  );
}

function DecisionCell({ row: r, data, qkey }: { row: ReconRow; data: Gstr2b; qkey: readonly unknown[] }) {
  const qc = useQueryClient();
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const decide = async (decision: ReconDecision | null) => {
    setBusy(true);
    try {
      const v = await api.decideGstr2b(data.registrationId, data.period, { rowKey: r.key, decision });
      qc.setQueryData(qkey, v);
      void qc.invalidateQueries({ queryKey: keys.report("gst", "centre") });
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), "error");
    } finally {
      setBusy(false);
    }
  };
  if (busy) return <Loader2 size={15} className="ml-auto animate-spin text-muted" />;
  if (r.decision)
    return (
      <div className="flex flex-col items-end gap-0.5">
        <Badge tone={r.decision === "accepted" ? "success" : "neutral"}>{r.decision === "accepted" ? "Accepted" : "Deferred"}</Badge>
        <button type="button" onClick={() => void decide(null)} className="text-[12px] text-muted hover:text-fg">
          Undo
        </button>
      </div>
    );
  const acceptLabel = r.status === "duplicate" ? "Acknowledge" : r.status === "likely" || r.status === "mismatch" ? "Accept" : "Accept as is";
  return (
    <div className="flex justify-end gap-1">
      {r.status !== "duplicate" && (
        <Button variant="ghost" onClick={() => void decide("deferred")}>
          Defer
        </Button>
      )}
      <Button onClick={() => void decide("accepted")}>{acceptLabel}</Button>
    </div>
  );
}

function ImportButton({ data, primary }: { data: Gstr2b; primary: boolean }) {
  const input = useRef<HTMLInputElement>(null);
  const qc = useQueryClient();
  const toast = useToast();
  const navigate = useNavigate();
  const [busy, setBusy] = useState(false);
  const pick = async (file: File | undefined) => {
    if (!file) return;
    setBusy(true);
    try {
      const r = await api.importGstr2b(data.registrationId, file.name, new Uint8Array(await file.arrayBuffer()));
      void qc.invalidateQueries({ queryKey: keys.report("gst") });
      toast(`Imported ${r.documentCount} documents for ${r.period}${r.replaced ? " (replaced the earlier import)" : ""}`, "success");
      if (r.period !== data.period) navigate(`/gst/gstr2b/${r.period}?reg=${data.registrationId}`);
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), "error");
    } finally {
      setBusy(false);
      if (input.current) input.current.value = "";
    }
  };
  return (
    <>
      <input ref={input} type="file" accept=".json,application/json" className="hidden" onChange={(e) => void pick(e.target.files?.[0])} />
      <Button variant={primary ? "primary" : undefined} disabled={busy} onClick={() => input.current?.click()}>
        {busy ? <Loader2 size={15} className="animate-spin" /> : <Upload size={15} />} {data.statement ? "Import again" : "Import GSTR-2B"}
      </Button>
    </>
  );
}

function ExportButton({ data }: { data: Gstr2b }) {
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const run = async () => {
    if (!runningInTauri) {
      toast("Saving the Excel report runs in the desktop app.", "info");
      return;
    }
    setBusy(true);
    try {
      const f = await api.saveGstr2bXlsx(data.registrationId, data.period);
      if (f) toast(`Saved ${f.fileName}`, "success", { label: "Open", onClick: () => void api.openSavedFile(f.path) });
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), "error");
    } finally {
      setBusy(false);
    }
  };
  return (
    <Button disabled={busy} onClick={() => void run()}>
      {busy ? <Loader2 size={15} className="animate-spin" /> : <Download size={15} />} Excel report
    </Button>
  );
}
