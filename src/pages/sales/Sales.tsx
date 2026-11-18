import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate } from "react-router";
import clsx from "clsx";
import { Plus, Receipt } from "lucide-react";
import { api, keys, type SalesSummary } from "../../lib/api";
import { formatPaise } from "../../lib/format";
import { STATUS, shortDate } from "../../lib/sales";
import { Badge, Button, Card, EmptyState, PageHeader, Segmented } from "../../components/ui";
import { SearchField } from "../../components/SearchField";

type Filter = "all" | "drafts" | "unpaid" | "overdue" | "cancelled";

const MATCH: Record<Filter, (d: SalesSummary) => boolean> = {
  all: () => true,
  drafts: (d) => d.displayStatus === "draft",
  unpaid: (d) => d.docType !== "credit_note" && ["issued", "overdue", "part_paid"].includes(d.displayStatus),
  overdue: (d) => d.displayStatus === "overdue",
  cancelled: (d) => d.displayStatus === "cancelled",
};

export function Sales() {
  const navigate = useNavigate();
  const { data: profile } = useQuery({ queryKey: keys.profile, queryFn: api.businessProfile });
  const { data: docs = [], isLoading } = useQuery({ queryKey: keys.sales, queryFn: api.listSales });
  const [filter, setFilter] = useState<Filter>("all");
  const [q, setQ] = useState("");

  const shown = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return docs.filter(
      (d) => MATCH[filter](d) && (!needle || [d.number, d.partyName, d.partyGstin].some((v) => v?.toLowerCase().includes(needle))),
    );
  }, [docs, filter, q]);

  const ready = !!profile?.organisation;
  const newButton = (
    <Link to="/sales/new">
      <Button variant="primary">
        <Plus size={15} /> New invoice
      </Button>
    </Link>
  );

  return (
    <div>
      <PageHeader title="Sales" subtitle="Tax invoices, bills of supply and credit notes." actions={ready && docs.length > 0 ? newButton : undefined} />
      {!ready && profile ? (
        <EmptyState
          icon={<Receipt size={22} />}
          title="Set up your business first"
          body="Invoices carry your GSTIN and number series, so set those up before your first sale."
          actions={
            <Link to="/setup">
              <Button variant="primary">Set up business</Button>
            </Link>
          }
        />
      ) : !isLoading && docs.length === 0 ? (
        <EmptyState
          icon={<Receipt size={22} />}
          title="Create your first invoice"
          body="Pick a customer, add items, and GST is worked out for you. Issuing it numbers the invoice and posts it to your books."
          actions={newButton}
        />
      ) : (
        <>
          <div className="mb-4 flex items-center justify-between gap-4">
            <Segmented<Filter>
              label="Show"
              value={filter}
              onChange={setFilter}
              options={(["all", "drafts", "unpaid", "overdue", "cancelled"] as Filter[]).map((f) => ({
                value: f,
                label: f === "all" ? "All" : f.charAt(0).toUpperCase() + f.slice(1),
                count: f === "all" ? undefined : docs.filter(MATCH[f]).length,
              }))}
            />
            <SearchField value={q} onChange={setQ} placeholder="Search number, customer, GSTIN" />
          </div>
          <Card className="overflow-hidden">
            <table className="w-full text-[13px]">
              <thead>
                <tr className="border-b border-line bg-surface-2 text-left text-[12px] text-muted">
                  <th className="px-4 py-2 font-medium">Number</th>
                  <th className="px-4 py-2 font-medium">Date</th>
                  <th className="px-4 py-2 font-medium">Customer</th>
                  <th className="px-4 py-2 font-medium">Status</th>
                  <th className="px-4 py-2 font-medium">Due</th>
                  <th className="px-4 py-2 text-right font-medium">Amount</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-line">
                {shown.map((d) => {
                  const open = () => navigate(d.status === "draft" ? `/sales/${d.id}/edit` : `/sales/${d.id}`);
                  const credit = d.docType === "credit_note";
                  return (
                    <tr
                      key={d.id}
                      tabIndex={0}
                      onClick={open}
                      onKeyDown={(e) => e.key === "Enter" && open()}
                      className={clsx("cursor-pointer hover:bg-hover focus-visible:bg-hover focus-visible:outline-none", d.status === "cancelled" && "text-muted")}
                    >
                      <td className="px-4 py-2.5">
                        <span className={clsx("num font-medium tracking-[0.02em]", d.status === "cancelled" && "line-through")}>
                          {d.number ?? <span className="font-normal text-subtle">Draft</span>}
                        </span>
                        {credit && <span className="ml-2"><Badge>Credit note</Badge></span>}
                        {d.docType === "bill_of_supply" && <span className="ml-2"><Badge>Bill of supply</Badge></span>}
                      </td>
                      <td className="num px-4 py-2.5 text-muted">{shortDate(d.docDate)}</td>
                      <td className="px-4 py-2.5">{d.partyName}</td>
                      <td className="px-4 py-2.5">
                        <Badge tone={STATUS[d.displayStatus].tone}>{STATUS[d.displayStatus].label}</Badge>
                      </td>
                      <td className="num px-4 py-2.5 text-muted">{credit ? "—" : shortDate(d.dueDate)}</td>
                      <td className="num px-4 py-2.5 text-right font-medium">
                        {credit ? "−" : ""}
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
