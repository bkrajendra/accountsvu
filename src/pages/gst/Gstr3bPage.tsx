import { useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "react-router";
import clsx from "clsx";
import { ArrowLeft, Download, Loader2 } from "lucide-react";
import { api, keys, runningInTauri, type Gstr3b, type Gstr3bRow, type HeadPayment, type Heads } from "../../lib/api";
import { formatPaise } from "../../lib/format";
import { shortDate } from "../../lib/sales";
import { Badge, Button, Card, Segmented } from "../../components/ui";
import { useToast } from "../../components/Toast";
import { Amount, Loading } from "../reports/common";
import { statusBadge, useReturnScope } from "./ReturnCentre";
import { Exceptions, ReturnSwitch } from "./Gstr1Page";
import { FilingPanel, monthLabel, useFiling } from "./FilingPanel";

const sum = (h: Heads) => h.igst + h.cgst + h.sgst + h.cess;

export function Gstr3bPage() {
  const { period = "" } = useParams();
  const scope = useReturnScope();
  const { data: centre } = useQuery({
    queryKey: keys.report("gst", "centre", "", ""),
    queryFn: () => api.returnCentre(null, null),
    enabled: !scope.reg,
  });
  const reg = scope.reg ?? centre?.registrationId ?? null;
  const { data, error } = useQuery({
    queryKey: keys.report("gst", "gstr3b", reg ?? "", period),
    queryFn: () => api.gstr3b(reg!, period),
    enabled: !!reg,
  });
  const { data: filing } = useFiling(reg, period, "gstr3b");

  if (error) return <Card className="px-4 py-10 text-center text-[13px] text-danger">{error instanceof Error ? error.message : String(error)}</Card>;
  if (!data) return <Loading />;

  const payable = sum(data.liability) + sum(data.reverseCharge);
  return (
    <div>
      <Link to={`/gst?reg=${data.registrationId}`} className="mb-2 inline-flex items-center gap-1 text-[13px] text-muted hover:text-fg">
        <ArrowLeft size={14} /> GST returns
      </Link>
      <div className="mb-5 flex items-end justify-between gap-4">
        <div>
          <ReturnSwitch current="gstr3b" period={data.period} reg={data.registrationId} />
          <h1 className="text-display">{data.label}</h1>
          <p className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-[14px] text-muted">
            <span className="num text-fg">{data.gstin}</span> · {data.stateName} · {statusBadge(data.status, data.dueDate, filing?.filing)}
          </p>
        </div>
        <ExportButton data={data} />
      </div>

      <FilingPanel reg={data.registrationId} period={data.period} returnType="gstr3b" />

      {data.exceptions.length > 0 && <Exceptions list={data.exceptions} />}

      <div className="mb-5 grid grid-cols-3 gap-4">
        <Stat label="Tax payable" value={payable} note={sum(data.reverseCharge) ? `incl. ${formatPaise(sum(data.reverseCharge))} reverse charge` : "on outward supplies"} />
        <Stat
          label="Credit available"
          value={sum(data.creditAvailable)}
          note={sum(data.creditBroughtForward) ? `net ITC ${formatPaise(sum(data.itcNet))} + ${formatPaise(sum(data.creditBroughtForward))} brought forward` : "net ITC this month"}
        />
        <Stat label="Cash to pay" value={data.offset.cashTotal} strong note={sum(data.offset.carryForward) ? `${formatPaise(sum(data.offset.carryForward))} credit carried forward` : "proposal, rule 88A order"} />
      </div>

      <Section title="3.1 Outward supplies and inward supplies liable to reverse charge">
        <TaxTable rows={data.outward} taxable />
      </Section>

      <Section title="3.2 Inter-state supplies to unregistered persons and composition taxpayers">
        <table className="w-full text-[13px]">
          <thead>
            <tr className={HEAD}>
              <Th>Place of supply</Th>
              <Th right>Unregistered: taxable value</Th>
              <Th right>IGST</Th>
              <Th right>Composition: taxable value</Th>
              <Th right>IGST</Th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {data.interState.map((r) => (
              <tr key={r.placeOfSupply}>
                <td className="px-4 py-2">
                  <span className="num text-muted">{String(r.placeOfSupply).padStart(2, "0")}</span> {r.placeName}
                </td>
                <td className="px-3 py-2 text-right"><Amount paise={r.unregisteredTaxablePaise} /></td>
                <td className="px-3 py-2 text-right"><Amount paise={r.unregisteredIgstPaise} /></td>
                <td className="px-3 py-2 text-right"><Amount paise={r.compositionTaxablePaise} /></td>
                <td className="px-4 py-2 text-right"><Amount paise={r.compositionIgstPaise} /></td>
              </tr>
            ))}
            {data.interState.length === 0 && <Empty cols={5} text="No inter-state supplies to unregistered or composition persons." />}
          </tbody>
        </table>
      </Section>

      <Section title="4 Eligible ITC">
        <table className="w-full text-[13px]">
          <TaxHead />
          <tbody className="divide-y divide-line">
            <Group label="(A) ITC available (whether in full or part)" />
            {data.itcAvailable.map((r) => <TaxRow key={`a${r.key}`} r={r} indent />)}
            <Group label="(B) ITC reversed" />
            {data.itcReversed.map((r) => <TaxRow key={`b${r.key}`} r={r} indent />)}
            <tr className="bg-surface-2 font-semibold">
              <td className="px-4 py-2">(C) Net ITC available (A) − (B)</td>
              <HeadCells h={data.itcNet} zero />
            </tr>
            <Group label="(D) Other details" />
            {data.itcOther.map((r) => <TaxRow key={`d${r.key}`} r={r} indent />)}
          </tbody>
        </table>
      </Section>

      <Section title="5 Exempt, nil-rated and non-GST inward supplies">
        <table className="w-full text-[13px]">
          <thead>
            <tr className={HEAD}>
              <Th>Nature of supplies</Th>
              <Th right>Inter-state</Th>
              <Th right>Intra-state</Th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {data.inwardExempt.map((r) => (
              <tr key={r.key}>
                <td className="px-4 py-2">{r.label}</td>
                <td className="px-3 py-2 text-right"><Amount paise={r.interStatePaise} zero /></td>
                <td className="px-4 py-2 text-right"><Amount paise={r.intraStatePaise} zero /></td>
              </tr>
            ))}
          </tbody>
        </table>
      </Section>

      <Section
        title="6.1 Payment of tax"
        note="A proposal in the rule 88A order: IGST credit first against IGST, then CGST and SGST; CGST and SGST credit pay IGST only after that; reverse charge is paid in cash. Adjust it on the portal if you prefer another split."
      >
        <table className="w-full text-[13px]">
          <thead>
            <tr className={HEAD}>
              <Th>Tax head</Th>
              <Th right>Payable</Th>
              <Th right>IGST credit</Th>
              <Th right>CGST credit</Th>
              <Th right>SGST/UTGST credit</Th>
              <Th right>Cess credit</Th>
              <Th right>Cash</Th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            <PayRow label="IGST" p={data.offset.igst} />
            <PayRow label="CGST" p={data.offset.cgst} />
            <PayRow label="SGST/UTGST" p={data.offset.sgst} />
            <PayRow label="Cess" p={data.offset.cess} />
            {sum(data.offset.reverseCharge) > 0 && (
              <tr>
                <td className="px-4 py-2">
                  Reverse charge <span className="text-muted">(cash only)</span>
                </td>
                <td className="px-3 py-2 text-right"><Amount paise={sum(data.offset.reverseCharge)} /></td>
                <td colSpan={4} />
                <td className="px-4 py-2 text-right"><Amount paise={sum(data.offset.reverseCharge)} /></td>
              </tr>
            )}
          </tbody>
          <tfoot>
            <tr className="border-t border-line-strong bg-surface-2 font-semibold">
              <td className="px-4 py-2.5">Cash to pay</td>
              <td colSpan={5} />
              <td className="px-4 py-2.5 text-right"><Amount paise={data.offset.cashTotal} zero /></td>
            </tr>
          </tfoot>
        </table>
        <div className="grid grid-cols-2 gap-px border-t border-line bg-line text-[12.5px]">
          <HeadsNote label="Credit brought forward (AccountsVU estimate)" h={data.creditBroughtForward} />
          <HeadsNote label="Credit carried forward" h={data.offset.carryForward} />
        </div>
      </Section>

      <div className="mb-5 grid grid-cols-2 gap-5">
        <Rule42Card data={data} />
        <CrossCheck data={data} />
      </div>

      <ItcLines data={data} />
      <p className="mt-3 text-[12px] text-subtle">
        From issued sales and recorded purchases dated {shortDate(data.from)}–{shortDate(data.to)}. ITC follows each line's review; pending lines are not claimed.
      </p>
    </div>
  );
}

const HEAD = "border-b border-line bg-surface-2 text-left text-[12px] text-muted";

function Th({ children, right }: { children?: ReactNode; right?: boolean }) {
  return <th className={clsx("px-3 py-2 font-medium first:pl-4 last:pr-4", right && "text-right")}>{children}</th>;
}

function Empty({ cols, text }: { cols: number; text: string }) {
  return (
    <tr>
      <td colSpan={cols} className="px-4 py-6 text-center text-muted">
        {text}
      </td>
    </tr>
  );
}

function Section({ title, note, children }: { title: string; note?: string; children: ReactNode }) {
  return (
    <Card className="mb-5 overflow-hidden">
      <div className="border-b border-line px-4 py-2.5">
        <h2 className="text-[13.5px] font-semibold">{title}</h2>
        {note && <p className="mt-0.5 text-[12px] text-muted">{note}</p>}
      </div>
      {children}
    </Card>
  );
}

function Stat({ label, value, note, strong }: { label: string; value: number; note: string; strong?: boolean }) {
  return (
    <Card className="px-4 py-3">
      <div className="text-[12px] font-medium text-muted">{label}</div>
      <div className={clsx("num mt-1 text-[22px] font-semibold tracking-tight", strong && "text-accent")}>{formatPaise(value)}</div>
      <div className="mt-0.5 text-[12px] text-muted">{note}</div>
    </Card>
  );
}

function HeadCells({ h, zero }: { h: Heads; zero?: boolean }) {
  return (
    <>
      <td className="px-3 py-2 text-right"><Amount paise={h.igst} zero={zero} /></td>
      <td className="px-3 py-2 text-right"><Amount paise={h.cgst} zero={zero} /></td>
      <td className="px-3 py-2 text-right"><Amount paise={h.sgst} zero={zero} /></td>
      <td className="px-4 py-2 text-right"><Amount paise={h.cess} zero={zero} /></td>
    </>
  );
}

function TaxHead({ taxable }: { taxable?: boolean }) {
  return (
    <thead>
      <tr className={HEAD}>
        <Th>Details</Th>
        {taxable && <Th right>Taxable value</Th>}
        <Th right>IGST</Th>
        <Th right>CGST</Th>
        <Th right>SGST/UTGST</Th>
        <Th right>Cess</Th>
      </tr>
    </thead>
  );
}

function TaxRow({ r, taxable, indent }: { r: Gstr3bRow; taxable?: boolean; indent?: boolean }) {
  const empty = !r.taxablePaise && !sum(r.tax);
  return (
    <tr className={clsx(empty && "text-muted")}>
      <td className={clsx("py-2 pr-3", indent ? "pl-8" : "pl-4")}>{r.label}</td>
      {taxable && <td className="px-3 py-2 text-right"><Amount paise={r.taxablePaise} /></td>}
      <HeadCells h={r.tax} />
    </tr>
  );
}

function TaxTable({ rows, taxable }: { rows: Gstr3bRow[]; taxable?: boolean }) {
  return (
    <table className="w-full text-[13px]">
      <TaxHead taxable={taxable} />
      <tbody className="divide-y divide-line">
        {rows.map((r) => <TaxRow key={r.key} r={r} taxable={taxable} />)}
      </tbody>
    </table>
  );
}

function Group({ label }: { label: string }) {
  return (
    <tr className="bg-surface-2/60 text-[12.5px] font-semibold">
      <td colSpan={5} className="px-4 py-1.5">
        {label}
      </td>
    </tr>
  );
}

function PayRow({ label, p }: { label: string; p: HeadPayment }) {
  return (
    <tr className={clsx(!p.liability && "text-muted")}>
      <td className="px-4 py-2">{label}</td>
      <td className="px-3 py-2 text-right"><Amount paise={p.liability} /></td>
      <td className="px-3 py-2 text-right"><Amount paise={p.byCredit.igst} /></td>
      <td className="px-3 py-2 text-right"><Amount paise={p.byCredit.cgst} /></td>
      <td className="px-3 py-2 text-right"><Amount paise={p.byCredit.sgst} /></td>
      <td className="px-3 py-2 text-right"><Amount paise={p.byCredit.cess} /></td>
      <td className="px-4 py-2 text-right font-medium"><Amount paise={p.cash} /></td>
    </tr>
  );
}

function HeadsNote({ label, h }: { label: string; h: Heads }) {
  const parts = [
    ["IGST", h.igst],
    ["CGST", h.cgst],
    ["SGST", h.sgst],
    ["Cess", h.cess],
  ] as const;
  const shown = parts.filter(([, v]) => v);
  return (
    <div className="bg-surface px-4 py-2">
      <span className="text-muted">{label}: </span>
      {shown.length ? shown.map(([k, v]) => `${k} ${formatPaise(v)}`).join(" · ") : "none"}
    </div>
  );
}

function Rule42Card({ data }: { data: Gstr3b }) {
  const r = data.rule42;
  return (
    <Card className="overflow-hidden">
      <div className="border-b border-line px-4 py-2.5">
        <h2 className="text-[13.5px] font-semibold">Rule 42 common credit</h2>
        <p className="mt-0.5 text-[12px] text-muted">Monthly reversal of the exempt share of credit used for both taxable and exempt supplies. Trued up for the year.</p>
      </div>
      <dl className="divide-y divide-line text-[13px]">
        <Line label={`Common credit C2 (${r.commonLines} ${r.commonLines === 1 ? "line" : "lines"})`} value={sum(r.commonCredit)} />
        <Line label="Exempt, nil-rated and non-GST turnover E" value={r.exemptTurnoverPaise} />
        <Line label="Total turnover F" value={r.totalTurnoverPaise} />
        <Line label="Reversal D1 = C2 × E ÷ F, in 4B(1)" value={sum(r.reversal)} strong />
      </dl>
      {r.commonLines === 0 && (
        <p className="border-t border-line px-4 py-2 text-[12px] text-muted">Mark a purchase line as common credit in its ITC review to include it here.</p>
      )}
    </Card>
  );
}

function Line({ label, value, strong }: { label: string; value: number; strong?: boolean }) {
  return (
    <div className={clsx("flex justify-between gap-4 px-4 py-2", strong && "font-semibold")}>
      <dt className={clsx(!strong && "text-muted")}>{label}</dt>
      <dd className="num">{formatPaise(value)}</dd>
    </div>
  );
}

function CrossCheck({ data }: { data: Gstr3b }) {
  const ok = data.crossCheck.every((c) => !c.differencePaise);
  return (
    <Card className="overflow-hidden">
      <div className="flex items-center justify-between border-b border-line px-4 py-2.5">
        <h2 className="text-[13.5px] font-semibold">GSTR-1 cross-check</h2>
        {ok ? <Badge tone="success">Matches</Badge> : <Badge tone="danger">Differs</Badge>}
      </div>
      <table className="w-full text-[13px]">
        <thead>
          <tr className={HEAD}>
            <Th />
            <Th right>GSTR-1</Th>
            <Th right>GSTR-3B</Th>
            <Th right>Difference</Th>
          </tr>
        </thead>
        <tbody className="divide-y divide-line">
          {data.crossCheck.map((c) => (
            <tr key={c.label}>
              <td className="px-4 py-1.5">{c.label}</td>
              <td className="px-3 py-1.5 text-right"><Amount paise={c.gstr1Paise} zero /></td>
              <td className="px-3 py-1.5 text-right"><Amount paise={c.gstr3bPaise} zero /></td>
              <td className={clsx("px-4 py-1.5 text-right", c.differencePaise && "font-semibold text-danger")}>
                {c.differencePaise ? formatPaise(c.differencePaise) : "—"}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </Card>
  );
}

type LineFilter = "claimed" | "pending" | "all";

function ItcLines({ data }: { data: Gstr3b }) {
  const navigate = useNavigate();
  const [filter, setFilter] = useState<LineFilter>("all");
  const pending = data.itcLines.filter((l) => l.status === "pending").length;
  const rows = data.itcLines.filter((l) => filter === "all" || (filter === "pending" ? l.status === "pending" : l.status !== "pending"));
  return (
    <Card className="overflow-hidden">
      <div className="flex items-center justify-between gap-3 border-b border-line px-4 py-2">
        <h2 className="text-[13.5px] font-semibold">ITC by purchase line</h2>
        <Segmented<LineFilter>
          label="Show"
          value={filter}
          onChange={setFilter}
          options={[
            { value: "all", label: "All", count: data.itcLines.length },
            { value: "claimed", label: "Reviewed", count: data.itcLines.length - pending },
            { value: "pending", label: "Pending", count: pending },
          ]}
        />
      </div>
      <table className="w-full text-[13px]">
        <thead>
          <tr className={HEAD}>
            <Th>Supplier invoice</Th>
            <Th>Line</Th>
            <Th>Table</Th>
            <Th>ITC</Th>
            <Th right>GST</Th>
            <Th right>Claimed</Th>
          </tr>
        </thead>
        <tbody className="divide-y divide-line">
          {rows.map((l, i) => {
            const go = () => navigate(`/purchases/${l.documentId}`);
            return (
              <tr key={i} tabIndex={0} onClick={go} onKeyDown={(e) => e.key === "Enter" && go()} className="cursor-pointer align-top hover:bg-hover">
                <td className="px-4 py-2">
                  <span className="num text-accent">{l.supplierInvoice ?? "—"}</span>
                  <div className="text-[12px] text-muted">
                    {shortDate(l.docDate)} · {l.partyName}
                  </div>
                </td>
                <td className="px-3 py-2">
                  {l.description}
                  {l.reason && <div className="text-[12px] text-muted">{l.reason}</div>}
                </td>
                <td className="num px-3 py-2 text-muted">{l.table === "pending" ? "—" : l.table}</td>
                <td className="px-3 py-2">
                  <span className="flex flex-wrap gap-1">
                    <Badge tone={l.status === "eligible" ? "success" : l.status === "pending" ? "warning" : l.status === "partial" ? "accent" : "neutral"}>
                      {{ eligible: "Eligible", partial: "Partial", ineligible: "Not eligible", pending: "Pending", na: "—" }[l.status]}
                    </Badge>
                    {l.common && <Badge tone="accent">Common</Badge>}
                    {l.reverseCharge && <Badge>RCM</Badge>}
                    {l.movedFrom && <Badge tone="accent">From {monthLabel(l.movedFrom)}</Badge>}
                  </span>
                </td>
                <td className="px-3 py-2 text-right"><Amount paise={sum(l.tax)} /></td>
                <td className="px-4 py-2 text-right font-medium"><Amount paise={sum(l.eligible)} zero={l.status !== "pending"} /></td>
              </tr>
            );
          })}
          {rows.length === 0 && <Empty cols={6} text="No purchase lines with GST this month." />}
        </tbody>
      </table>
    </Card>
  );
}

function ExportButton({ data }: { data: Gstr3b }) {
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const run = async () => {
    if (!runningInTauri) {
      toast("Saving the Excel workpaper runs in the desktop app.", "info");
      return;
    }
    setBusy(true);
    try {
      const f = await api.saveGstr3bXlsx(data.registrationId, data.period);
      if (f) toast(`Saved ${f.fileName}`, "success", { label: "Show in folder", onClick: () => void api.openSavedFile(f.path) });
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), "error");
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="flex shrink-0 flex-col items-end gap-1">
      <Button disabled={busy} onClick={() => void run()}>
        {busy ? <Loader2 size={15} className="animate-spin" /> : <Download size={15} />} Excel workpaper
      </Button>
      <span className="text-[12px] text-muted">Tables, payment proposal and ITC lines</span>
    </div>
  );
}
