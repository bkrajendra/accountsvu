import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router";
import clsx from "clsx";
import { Plus, ShoppingBag, Wallet } from "lucide-react";
import { api, keys, type PurchaseSummary } from "../../lib/api";
import { formatPaise } from "../../lib/format";
import { shortDate } from "../../lib/sales";
import { PURCHASE_STATUS } from "../../lib/purchases";
import { Badge, Button, Card, EmptyState, PageHeader, Segmented } from "../../components/ui";
import { SearchField } from "../../components/SearchField";

type Filter = "all" | "drafts" | "unpaid" | "itc" | "cancelled";

const MATCH: Record<Filter, (d: PurchaseSummary) => boolean> = {
  all: () => true,
  drafts: (d) => d.displayStatus === "draft",
  unpaid: (d) => ["unpaid", "overdue", "part_paid"].includes(d.displayStatus),
  itc: (d) => d.itcPendingLines > 0,
  cancelled: (d) => d.displayStatus === "cancelled",
};

const LABEL: Record<Filter, string> = { all: "All", drafts: "Drafts", unpaid: "Unpaid", itc: "ITC to review", cancelled: "Cancelled" };

function reference(d: PurchaseSummary): string {
  if (d.docType === "debit_note") return d.number ?? "Draft debit note";
  return d.supplierInvoiceNumber ?? (d.docType === "expense" ? "Expense" : "—");
}

export function Purchases() {
  const navigate = useNavigate();
  const { data: profile } = useQuery({ queryKey: keys.profile, queryFn: api.businessProfile });
  const { data: docs = [], isLoading } = useQuery({ queryKey: keys.purchases, queryFn: api.listPurchases });
  const [filter, setFilter] = useState<Filter>("all");
  const [q, setQ] = useState("");

  const shown = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return docs.filter(
      (d) =>
        MATCH[filter](d) &&
        (!needle || [d.supplierInvoiceNumber, d.number, d.partyName, d.partyGstin, d.notes].some((v) => v?.toLowerCase().includes(needle))),
    );
  }, [docs, filter, q]);

  const ready = !!profile?.organisation;
  const actions = (
    <>
      <Link to="/purchases/new?type=expense">
        <Button>
          <Wallet size={15} /> New expense
        </Button>
      </Link>
      <Link to="/purchases/new?type=bill">
        <Button variant="primary">
          <Plus size={15} /> New bill
        </Button>
      </Link>
    </>
  );

  return (
    <div>
      <PageHeader title="Purchases" subtitle="Supplier bills, expenses and debit notes, with input tax credit review." actions={ready && docs.length > 0 ? actions : undefined} />
      {!ready && profile ? (
        <EmptyState
          icon={<ShoppingBag size={22} />}
          title="Set up your business first"
          body="Purchases are booked against your business and its GSTIN."
          actions={
            <Link to="/setup">
              <Button variant="primary">Set up business</Button>
            </Link>
          }
        />
      ) : !isLoading && docs.length === 0 ? (
        <EmptyState
          icon={<ShoppingBag size={22} />}
          title="Record your first purchase"
          body="Enter a supplier's bill as it is printed, or a quick expense paid in cash. You decide the input tax credit for each line."
          actions={actions}
        />
      ) : (
        <>
          <div className="mb-4 flex items-center justify-between gap-4">
            <Segmented<Filter>
              label="Show"
              value={filter}
              onChange={setFilter}
              options={(Object.keys(LABEL) as Filter[]).map((f) => ({ value: f, label: LABEL[f], count: f === "all" ? undefined : docs.filter(MATCH[f]).length }))}
            />
            <SearchField value={q} onChange={setQ} placeholder="Search supplier, invoice number" />
          </div>
          <Card className="overflow-hidden">
            <table className="w-full text-[13px]">
              <thead>
                <tr className="border-b border-line bg-surface-2 text-left text-[12px] text-muted">
                  <th className="px-4 py-2 font-medium">Date</th>
                  <th className="px-4 py-2 font-medium">Supplier</th>
                  <th className="px-4 py-2 font-medium">Reference</th>
                  <th className="px-4 py-2 font-medium">Status</th>
                  <th className="px-4 py-2 font-medium">ITC</th>
                  <th className="px-4 py-2 text-right font-medium">Amount</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-line">
                {shown.map((d) => {
                  const open = () => navigate(d.status === "draft" ? `/purchases/${d.id}/edit` : `/purchases/${d.id}`);
                  const st = PURCHASE_STATUS[d.displayStatus];
                  return (
                    <tr
                      key={d.id}
                      tabIndex={0}
                      onClick={open}
                      onKeyDown={(e) => e.key === "Enter" && open()}
                      className={clsx("cursor-pointer hover:bg-hover focus-visible:bg-hover focus-visible:outline-none", d.status === "cancelled" && "text-muted")}
                    >
                      <td className="num px-4 py-2.5 text-muted">{shortDate(d.docDate)}</td>
                      <td className="px-4 py-2.5">{d.partyName ?? <span className="text-muted">No supplier</span>}</td>
                      <td className="px-4 py-2.5">
                        <span className={clsx("num", d.status === "cancelled" && "line-through")}>{reference(d)}</span>
                        {d.docType !== "bill" && <span className="ml-2"><Badge>{d.docType === "expense" ? "Expense" : "Debit note"}</Badge></span>}
                      </td>
                      <td className="px-4 py-2.5">
                        <Badge tone={st.tone}>{st.label}</Badge>
                      </td>
                      <td className="px-4 py-2.5">{d.itcPendingLines > 0 ? <Badge tone="warning">{d.itcPendingLines} to review</Badge> : <span className="text-subtle">—</span>}</td>
                      <td className="num px-4 py-2.5 text-right font-medium">
                        {d.docType === "debit_note" ? "−" : ""}
                        {formatPaise(d.totalPaise)}
                      </td>
                    </tr>
                  );
                })}
                {shown.length === 0 && (
                  <tr>
                    <td colSpan={6} className="px-4 py-10 text-center text-muted">
                      Nothing here.
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </Card>
        </>
      )}
    </div>
  );
}
