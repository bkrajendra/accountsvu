import { useState, type ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useParams } from "react-router";
import clsx from "clsx";
import { AlertCircle, AlertTriangle, ArrowLeft, Download, Loader2 } from "lucide-react";
import { api, keys, runningInTauri, type Gstr1, type ReturnException, type ReturnSectionKey, type TaxTotals } from "../../lib/api";
import { formatPaise, formatRate } from "../../lib/format";
import { shortDate } from "../../lib/sales";
import { Badge, Button, Card, Segmented } from "../../components/ui";
import { useToast } from "../../components/Toast";
import { Amount, Loading } from "../reports/common";
import { statusBadge, useReturnScope } from "./ReturnCentre";
import { FilingPanel, useFiling } from "./FilingPanel";

type Tab = "b2b" | "b2cl" | "exp" | "b2cs" | "nil" | "cdn" | "hsn" | "docs";

const TAB_OF: Record<ReturnSectionKey, Tab> = {
  b2b: "b2b",
  b2cl: "b2cl",
  exp: "exp",
  b2cs: "b2cs",
  nil: "nil",
  cdnr: "cdn",
  cdnur: "cdn",
  hsn: "hsn",
  docs: "docs",
};

const docHref = (id: string) => `/sales/${id}`;

export function Gstr1Page() {
  const { period = "" } = useParams();
  const scope = useReturnScope();
  const { data: centre } = useQuery({
    queryKey: keys.report("gst", "centre", "", ""),
    queryFn: () => api.returnCentre(null, null),
    enabled: !scope.reg,
  });
  const reg = scope.reg ?? centre?.registrationId ?? null;
  const { data, error } = useQuery({
    queryKey: keys.report("gst", "gstr1", reg ?? "", period),
    queryFn: () => api.gstr1(reg!, period),
    enabled: !!reg,
  });
  const { data: filing } = useFiling(reg, period, "gstr1");
  const [tab, setTab] = useState<Tab | null>(null);

  if (error) return <Card className="px-4 py-10 text-center text-[13px] text-danger">{error instanceof Error ? error.message : String(error)}</Card>;
  if (!data) return <Loading />;

  const count = (keysIn: ReturnSectionKey[]) => data.sections.filter((s) => keysIn.includes(s.key)).reduce((a, s) => a + s.count, 0);
  const tabs: { value: Tab; label: string; count: number }[] = [
    { value: "b2b", label: "B2B", count: count(["b2b"]) },
    { value: "b2cl", label: "B2C large", count: count(["b2cl"]) },
    { value: "b2cs", label: "B2C small", count: data.b2cs.length },
    { value: "exp", label: "Exports", count: count(["exp"]) },
    { value: "nil", label: "Nil & exempt", count: data.nil.length },
    { value: "cdn", label: "Credit notes", count: count(["cdnr", "cdnur"]) },
    { value: "hsn", label: "HSN", count: data.hsn.length },
    { value: "docs", label: "Documents", count: data.series.length },
  ];
  const current = tab ?? tabs.find((t) => t.count > 0)?.value ?? "b2b";
  const errors = data.exceptions.filter((e) => e.severity === "error").length;

  return (
    <div>
      <Link to={`/gst?reg=${data.registrationId}`} className="mb-2 inline-flex items-center gap-1 text-[13px] text-muted hover:text-fg">
        <ArrowLeft size={14} /> GST returns
      </Link>
      <div className="mb-5 flex items-end justify-between gap-4">
        <div>
          <ReturnSwitch current="gstr1" period={data.period} reg={data.registrationId} />
          <h1 className="text-display">{data.label}</h1>
          <p className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-[14px] text-muted">
            <span className="num text-fg">{data.gstin}</span> · {data.stateName} · {statusBadge(data.status, data.dueDate, filing?.filing)}
          </p>
        </div>
        {!filing?.filing && <ExportButton data={data} errors={errors} />}
      </div>

      <FilingPanel reg={data.registrationId} period={data.period} returnType="gstr1" />

      {data.exceptions.length > 0 && <Exceptions list={data.exceptions} />}

      <Card className="mb-5 overflow-hidden">
        <table className="w-full text-[13px]">
          <thead>
            <tr className="border-b border-line bg-surface-2 text-left text-[12px] text-muted">
              <th className="w-[150px] px-4 py-2 font-medium">Table</th>
              <th className="px-3 py-2 font-medium">Section</th>
              <th className="px-3 py-2 text-right font-medium">Count</th>
              <th className="px-3 py-2 text-right font-medium">Taxable / amount</th>
              <th className="px-3 py-2 text-right font-medium">IGST</th>
              <th className="px-3 py-2 text-right font-medium">CGST</th>
              <th className="px-3 py-2 text-right font-medium">SGST/UTGST</th>
              <th className="px-4 py-2 text-right font-medium">Cess</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {data.sections.map((s) => {
              const credit = s.key === "cdnr" || s.key === "cdnur";
              const sign = credit ? -1 : 1;
              const amount = s.key === "nil" ? s.valuePaise : s.totals.taxablePaise;
              const go = () => setTab(TAB_OF[s.key]);
              return (
                <tr key={s.key} tabIndex={0} onClick={go} onKeyDown={(e) => e.key === "Enter" && go()} className={clsx("cursor-pointer hover:bg-hover", s.count === 0 && "text-muted")}>
                  <td className="num px-4 py-2 text-[12.5px] text-subtle">{s.table}</td>
                  <td className="px-3 py-2">{s.title}</td>
                  <td className="num px-3 py-2 text-right">{s.count || ""}</td>
                  <td className="px-3 py-2 text-right">
                    {s.key === "docs" ? "" : <Amount paise={sign * amount} />}
                  </td>
                  {s.key === "hsn" || s.key === "docs" || s.key === "nil" ? (
                    <td colSpan={4} className="px-4 py-2 text-right text-[12px] text-subtle">
                      {s.key === "hsn" && s.count > 0 && `tax ${formatPaise(s.totals.taxPaise)}`}
                    </td>
                  ) : (
                    <TaxCells t={s.totals} sign={sign} />
                  )}
                </tr>
              );
            })}
          </tbody>
          <tfoot>
            <tr className="border-t border-line-strong bg-surface-2 font-semibold">
              <td colSpan={2} className="px-4 py-2.5">Net output tax</td>
              <td />
              <td className="px-3 py-2.5 text-right"><Amount paise={data.totals.taxablePaise} zero /></td>
              <TaxCells t={data.totals} zero />
            </tr>
          </tfoot>
        </table>
      </Card>

      <div className="mb-3 overflow-x-auto">
        <Segmented<Tab> label="GSTR-1 table" value={current} onChange={setTab} options={tabs} />
      </div>
      <Card className="overflow-hidden">
        <TabBody tab={current} data={data} />
      </Card>
      <p className="mt-3 text-[12px] text-subtle">
        Placed with GST rules {data.rulePackVersion} · export format {data.formatVersion} · only issued documents dated {shortDate(data.from)}–
        {shortDate(data.to)} are included.
      </p>
    </div>
  );
}

function TaxCells({ t, sign = 1, zero }: { t: TaxTotals; sign?: number; zero?: boolean }) {
  return (
    <>
      <td className="px-3 py-2 text-right"><Amount paise={sign * t.igstPaise} zero={zero} /></td>
      <td className="px-3 py-2 text-right"><Amount paise={sign * t.cgstPaise} zero={zero} /></td>
      <td className="px-3 py-2 text-right"><Amount paise={sign * t.sgstPaise} zero={zero} /></td>
      <td className="px-4 py-2 text-right"><Amount paise={sign * t.cessPaise} zero={zero} /></td>
    </>
  );
}

function ExportButton({ data, errors }: { data: Gstr1; errors: number }) {
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const run = async () => {
    if (!runningInTauri) {
      toast("Saving the GSTR-1 file runs in the desktop app.", "info");
      return;
    }
    setBusy(true);
    try {
      const f = await api.saveGstr1Json(data.registrationId, data.period);
      if (f) toast(`Saved ${f.fileName}`, "success", { label: "Show in folder", onClick: () => void api.openSavedFile(f.path) });
    } catch (e) {
      toast(e instanceof Error ? e.message : String(e), "error");
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="flex shrink-0 flex-col items-end gap-1">
      <Button variant="primary" disabled={busy} onClick={() => void run()}>
        {busy ? <Loader2 size={15} className="animate-spin" /> : <Download size={15} />} Export JSON
      </Button>
      <span className={clsx("text-[12px]", errors ? "text-danger" : "text-muted")}>
        {errors ? `Fix ${errors} ${errors === 1 ? "error" : "errors"} first; the portal may reject the file` : "For the GST portal's offline upload"}
      </span>
    </div>
  );
}

export function Exceptions({ list }: { list: ReturnException[] }) {
  const navigate = useNavigate();
  const [all, setAll] = useState(false);
  const shown = all ? list : list.slice(0, 5);
  const errors = list.filter((e) => e.severity === "error").length;
  return (
    <Card className="mb-5 overflow-hidden">
      <div className="flex items-center gap-2 border-b border-line bg-surface-2 px-4 py-2 text-[13px] font-medium">
        {errors > 0 ? <AlertCircle size={15} className="text-danger" /> : <AlertTriangle size={15} className="text-warning" />}
        {errors > 0 ? `${errors} to fix` : "Nothing blocking"}
        {list.length - errors > 0 && <span className="font-normal text-muted">· {list.length - errors} to check</span>}
      </div>
      <ul className="divide-y divide-line text-[13px]">
        {shown.map((e, i) => (
          <li key={i} className="flex items-start gap-2.5 px-4 py-2">
            {e.severity === "error" ? (
              <AlertCircle size={14} className="mt-[3px] shrink-0 text-danger" aria-label="Error" />
            ) : (
              <AlertTriangle size={14} className="mt-[3px] shrink-0 text-warning" aria-label="Warning" />
            )}
            <span className="min-w-0 flex-1">{e.message}</span>
            {e.documentId && (
              <button type="button" className="shrink-0 font-medium text-accent hover:underline" onClick={() => navigate(docHref(e.documentId!))}>
                Open
              </button>
            )}
          </li>
        ))}
      </ul>
      {list.length > 5 && (
        <button type="button" onClick={() => setAll(!all)} className="w-full border-t border-line px-4 py-2 text-left text-[12.5px] font-medium text-accent hover:bg-hover">
          {all ? "Show fewer" : `Show all ${list.length}`}
        </button>
      )}
    </Card>
  );
}

function Th({ children, right, className }: { children?: ReactNode; right?: boolean; className?: string }) {
  return <th className={clsx("px-3 py-2 font-medium first:pl-4 last:pr-4", right && "text-right", className)}>{children}</th>;
}

function Empty({ cols, text }: { cols: number; text: string }) {
  return (
    <tr>
      <td colSpan={cols} className="px-4 py-10 text-center text-muted">
        {text}
      </td>
    </tr>
  );
}

function TabBody({ tab, data }: { tab: Tab; data: Gstr1 }) {
  const navigate = useNavigate();
  const head = "border-b border-line bg-surface-2 text-left text-[12px] text-muted";

  if (tab === "b2cs") {
    return (
      <table className="w-full text-[13px]">
        <thead>
          <tr className={head}>
            <Th>Place of supply</Th>
            <Th>Type</Th>
            <Th right>Rate</Th>
            <Th right>Taxable value</Th>
            <Th right>IGST</Th>
            <Th right>CGST</Th>
            <Th right>SGST/UTGST</Th>
            <Th right>Cess</Th>
          </tr>
        </thead>
        <tbody className="divide-y divide-line">
          {data.b2cs.map((r) => (
            <tr key={`${r.placeOfSupply}-${r.rateBp}-${r.intraState}`} className="align-top">
              <td className="px-4 py-2">
                <span className="num text-muted">{String(r.placeOfSupply).padStart(2, "0")}</span> {r.placeName}
                <div className="mt-0.5 flex flex-wrap gap-x-2 text-[12px]">
                  {r.documents.map((d) => (
                    <Link key={d.id} to={docHref(d.id)} className="num text-accent hover:underline">
                      {d.number}
                    </Link>
                  ))}
                </div>
              </td>
              <td className="px-3 py-2 text-muted">{r.intraState ? "Intra-state" : "Inter-state"}</td>
              <td className="num px-3 py-2 text-right">{formatRate(r.rateBp)}</td>
              <td className="px-3 py-2 text-right"><Amount paise={r.totals.taxablePaise} zero /></td>
              <TaxCells t={r.totals} />
            </tr>
          ))}
          {data.b2cs.length === 0 && <Empty cols={8} text="No B2C small supplies this month." />}
        </tbody>
      </table>
    );
  }

  if (tab === "nil") {
    return (
      <table className="w-full text-[13px]">
        <thead>
          <tr className={head}>
            <Th>Supply</Th>
            <Th right>Nil-rated</Th>
            <Th right>Exempt</Th>
            <Th right>Non-GST</Th>
          </tr>
        </thead>
        <tbody className="divide-y divide-line">
          {data.nil.map((n) => (
            <tr key={n.supply}>
              <td className="px-4 py-2">{n.label}</td>
              <td className="px-3 py-2 text-right"><Amount paise={n.nilPaise} zero /></td>
              <td className="px-3 py-2 text-right"><Amount paise={n.exemptPaise} zero /></td>
              <td className="px-4 py-2 text-right"><Amount paise={n.nonGstPaise} zero /></td>
            </tr>
          ))}
          {data.nil.length === 0 && <Empty cols={4} text="No nil-rated, exempt or non-GST supplies this month." />}
        </tbody>
      </table>
    );
  }

  if (tab === "hsn") {
    const groups: ("b2b" | "b2c")[] = ["b2b", "b2c"];
    return (
      <table className="w-full text-[13px]">
        <thead>
          <tr className={head}>
            <Th>HSN/SAC</Th>
            <Th>UQC</Th>
            <Th right>Quantity</Th>
            <Th right>Rate</Th>
            <Th right>Taxable value</Th>
            <Th right>IGST</Th>
            <Th right>CGST</Th>
            <Th right>SGST/UTGST</Th>
            <Th right>Cess</Th>
          </tr>
        </thead>
        {groups.map((g) => {
          const rows = data.hsn.filter((h) => h.group === g);
          if (!rows.length) return null;
          return (
            <tbody key={g} className="divide-y divide-line">
              <tr className="bg-surface-2/60 text-[12px] font-semibold text-muted">
                <td colSpan={9} className="px-4 py-1.5">
                  {g === "b2b" ? "To registered persons (B2B)" : "To unregistered persons (B2C)"}
                </td>
              </tr>
              {rows.map((h) => (
                <tr key={`${g}-${h.hsn}-${h.uqc}-${h.rateBp}`}>
                  <td className="px-4 py-2">
                    <span className="num font-medium">{h.hsn}</span> <span className="text-muted">{h.description}</span>
                  </td>
                  <td className="num px-3 py-2">{h.uqc}</td>
                  <td className="num px-3 py-2 text-right">{h.uqc === "NA" ? "" : h.quantity}</td>
                  <td className="num px-3 py-2 text-right">{formatRate(h.rateBp)}</td>
                  <td className="px-3 py-2 text-right"><Amount paise={h.totals.taxablePaise} zero /></td>
                  <TaxCells t={h.totals} />
                </tr>
              ))}
            </tbody>
          );
        })}
        {data.hsn.length === 0 && (
          <tbody>
            <Empty cols={9} text="No supplies this month." />
          </tbody>
        )}
      </table>
    );
  }

  if (tab === "docs") {
    return (
      <table className="w-full text-[13px]">
        <thead>
          <tr className={head}>
            <Th>Nature of document</Th>
            <Th>From</Th>
            <Th>To</Th>
            <Th right>Total</Th>
            <Th right>Cancelled</Th>
            <Th right>Net issued</Th>
          </tr>
        </thead>
        <tbody className="divide-y divide-line">
          {data.series.map((s, i) => (
            <tr key={i}>
              <td className="px-4 py-2">{s.label}</td>
              <td className="num px-3 py-2">{s.from}</td>
              <td className="num px-3 py-2">{s.to}</td>
              <td className="num px-3 py-2 text-right">{s.total}</td>
              <td className="num px-3 py-2 text-right">{s.cancelled || ""}</td>
              <td className="num px-4 py-2 text-right font-medium">{s.net}</td>
            </tr>
          ))}
          {data.series.length === 0 && <Empty cols={6} text="No documents issued this month." />}
        </tbody>
      </table>
    );
  }

  const sections = tab === "cdn" ? ["cdnr", "cdnur"] : [tab];
  const rows = data.documents.filter((r) => sections.includes(r.section));
  const showGstin = tab === "b2b" || tab === "cdn";
  const empty: Record<string, string> = {
    b2b: "No invoices to registered persons this month.",
    b2cl: "No B2C large invoices this month.",
    exp: "No exports this month.",
    cdn: "No credit notes reported in table 9B this month. Notes against B2C small invoices are netted in B2C small.",
  };
  return (
    <table className="w-full text-[13px]">
      <thead>
        <tr className={head}>
          <Th>{tab === "cdn" ? "Note" : "Invoice"}</Th>
          <Th>{showGstin ? "Recipient" : "Customer"}</Th>
          <Th>Place of supply</Th>
          <Th right>Value</Th>
          <Th right>Taxable value</Th>
          <Th right>IGST</Th>
          <Th right>CGST</Th>
          <Th right>SGST/UTGST</Th>
          <Th right>Cess</Th>
        </tr>
      </thead>
      <tbody className="divide-y divide-line">
        {rows.map((r) => {
          const go = () => navigate(docHref(r.id));
          return (
            <tr key={r.id} tabIndex={0} onClick={go} onKeyDown={(e) => e.key === "Enter" && go()} className="cursor-pointer align-top hover:bg-hover">
              <td className="px-4 py-2">
                <span className="num text-accent">{r.number}</span>
                <div className="text-[12px] text-muted">
                  {shortDate(r.docDate)}
                  {r.originalNumber && ` · against ${r.originalNumber}`}
                </div>
              </td>
              <td className="px-3 py-2">
                {r.partyName}
                <div className="flex flex-wrap items-center gap-1.5 text-[12px] text-muted">
                  {r.gstin && <span className="num">{r.gstin}</span>}
                  {r.kindLabel !== "Regular" && <Badge>{r.kindLabel}</Badge>}
                  {r.reverseCharge && <Badge tone="warning">Reverse charge</Badge>}
                  {tab === "cdn" && <Badge>{r.section === "cdnr" ? "Registered" : "Unregistered"}</Badge>}
                </div>
              </td>
              <td className="px-3 py-2 text-muted">
                <span className="num">{String(r.placeOfSupply).padStart(2, "0")}</span> {r.placeName}
              </td>
              <td className="px-3 py-2 text-right"><Amount paise={r.valuePaise} zero /></td>
              <td className="px-3 py-2 text-right"><Amount paise={r.totals.taxablePaise} zero /></td>
              <TaxCells t={r.totals} />
            </tr>
          );
        })}
        {rows.length === 0 && <Empty cols={9} text={empty[tab] ?? "Nothing this month."} />}
      </tbody>
    </table>
  );
}

/** GSTR-1 / GSTR-3B for the same month and GSTIN. */
export function ReturnSwitch({ current, period, reg }: { current: "gstr1" | "gstr3b" | "gstr2b"; period: string; reg: string }) {
  const items = [
    { id: "gstr1", label: "GSTR-1" },
    { id: "gstr3b", label: "GSTR-3B" },
    { id: "gstr2b", label: "GSTR-2B" },
  ] as const;
  return (
    <nav aria-label="Return" className="mb-1 flex gap-3 text-[12px] font-semibold uppercase tracking-[0.08em]">
      {items.map((i) =>
        i.id === current ? (
          <span key={i.id} aria-current="page" className="text-accent">
            {i.label}
          </span>
        ) : (
          <Link key={i.id} to={`/gst/${i.id}/${period}?reg=${reg}`} className="text-subtle hover:text-fg">
            {i.label}
          </Link>
        ),
      )}
    </nav>
  );
}
