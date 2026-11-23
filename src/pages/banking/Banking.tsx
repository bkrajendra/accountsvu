import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link, useNavigate, useSearchParams } from "react-router";
import clsx from "clsx";
import { ArrowDownLeft, ArrowUpRight, Banknote, BookOpen, GitCompareArrows, Landmark, Pencil, Plus } from "lucide-react";
import { api, keys, type MoneyAccount, type VoucherSummary } from "../../lib/api";
import { formatPaise } from "../../lib/format";
import { shortDate } from "../../lib/sales";
import { MODE_LABEL, VOUCHER_LABEL } from "../../lib/banking";
import { Badge, Button, Card, EmptyState, PageHeader, Segmented } from "../../components/ui";
import { SearchField } from "../../components/SearchField";
import { MoneyAccountDialog } from "./MoneyAccountDialog";

type Tab = "accounts" | "vouchers";

export function Banking() {
  const [params, setParams] = useSearchParams();
  const tab: Tab = params.get("tab") === "vouchers" ? "vouchers" : "accounts";
  const { data: profile } = useQuery({ queryKey: keys.profile, queryFn: api.businessProfile });
  const { data: accounts = [] } = useQuery({ queryKey: keys.moneyAccounts, queryFn: api.listMoneyAccounts });
  const { data: vouchers = [] } = useQuery({ queryKey: keys.vouchers, queryFn: api.listVouchers });
  const [editing, setEditing] = useState<MoneyAccount | "new" | null>(null);

  const ready = !!profile?.organisation;
  const total = accounts.reduce((a, m) => a + m.balancePaise, 0);
  const actions = (
    <>
      <Link to="/banking/vouchers/new?type=payment">
        <Button>
          <ArrowUpRight size={15} /> Record payment
        </Button>
      </Link>
      <Link to="/banking/vouchers/new?type=receipt">
        <Button variant="primary">
          <ArrowDownLeft size={15} /> Record receipt
        </Button>
      </Link>
    </>
  );

  if (!ready && profile) {
    return (
      <div>
        <PageHeader title="Banking" />
        <EmptyState
          icon={<Landmark size={22} />}
          title="Set up your business first"
          body="Cash and bank accounts are created with your chart of accounts."
          actions={
            <Link to="/setup">
              <Button variant="primary">Set up business</Button>
            </Link>
          }
        />
      </div>
    );
  }

  return (
    <div>
      <PageHeader
        title="Banking"
        subtitle={
          <>
            Money received and paid, cash and bank books, and statement reconciliation. Cash and bank together:{" "}
            <span className="num font-semibold text-fg">{formatPaise(total)}</span>
          </>
        }
        actions={actions}
      />
      <div className="mb-4 flex items-center justify-between gap-4">
        <Segmented<Tab>
          label="Show"
          value={tab}
          onChange={(t) => setParams(t === "accounts" ? {} : { tab: t }, { replace: true })}
          options={[
            { value: "accounts", label: "Accounts", count: accounts.length },
            { value: "vouchers", label: "Receipts & payments", count: vouchers.length },
          ]}
        />
        {tab === "accounts" && (
          <Button variant="ghost" onClick={() => setEditing("new")}>
            <Plus size={15} /> Add account
          </Button>
        )}
      </div>

      {tab === "accounts" ? (
        <div className="grid grid-cols-2 gap-4">
          {accounts.map((m) => (
            <AccountCard key={m.id} m={m} onEdit={() => setEditing(m)} />
          ))}
        </div>
      ) : (
        <VoucherList vouchers={vouchers} />
      )}

      {editing && <MoneyAccountDialog account={editing === "new" ? undefined : editing} onClose={() => setEditing(null)} />}
    </div>
  );
}

function AccountCard({ m, onEdit }: { m: MoneyAccount; onEdit: () => void }) {
  const Icon = m.kind === "cash" ? Banknote : Landmark;
  const detail = [m.bankName, m.accountNumber && `•••• ${m.accountNumber.slice(-4)}`, m.ifsc].filter(Boolean).join(" · ");
  const diff = m.statementBalancePaise != null ? m.balancePaise - m.statementBalancePaise : null;
  return (
    <Card className={clsx("flex flex-col p-5", !m.isActive && "opacity-60")}>
      <div className="flex items-start gap-3">
        <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">
          <Icon size={19} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <h2 className="truncate text-[15px] font-semibold">{m.name}</h2>
            {!m.isActive && <Badge>Closed</Badge>}
          </div>
          <p className="num truncate text-[12.5px] text-muted">{detail || (m.kind === "cash" ? "Cash" : "Bank account")} · Ledger {m.ledgerCode}</p>
        </div>
        <button type="button" aria-label={`Edit ${m.name}`} onClick={onEdit} className="pressable grid size-8 place-items-center rounded-lg text-muted hover:bg-hover hover:text-fg">
          <Pencil size={15} />
        </button>
      </div>
      <div className="mt-5">
        <div className="text-[11.5px] font-semibold uppercase tracking-[0.05em] text-muted">Balance in books</div>
        <div className={clsx("num mt-0.5 text-[26px] font-semibold tracking-[-0.01em]", m.balancePaise < 0 && "text-danger")}>{formatPaise(m.balancePaise)}</div>
      </div>
      {m.kind === "bank" && (
        <div className="mt-2 flex flex-wrap items-center gap-2 text-[12.5px] text-muted">
          {m.lastStatementDate ? (
            <>
              <span>Statement to {shortDate(m.lastStatementDate)}</span>
              {m.unmatchedLines > 0 ? <Badge tone="warning">{m.unmatchedLines} to reconcile</Badge> : <Badge tone="success">Reconciled</Badge>}
              {diff != null && diff !== 0 && <span className="num">Bank shows {formatPaise(m.statementBalancePaise)}</span>}
            </>
          ) : (
            <span>No statement imported yet</span>
          )}
        </div>
      )}
      <div className="mt-auto flex gap-2 pt-5">
        <Link to={`/banking/accounts/${m.id}`}>
          <Button>
            <BookOpen size={15} /> {m.kind === "cash" ? "Cash book" : "Bank book"}
          </Button>
        </Link>
        {m.kind === "bank" && (
          <Link to={`/banking/accounts/${m.id}/reconcile`}>
            <Button>
              <GitCompareArrows size={15} /> Reconcile
            </Button>
          </Link>
        )}
      </div>
    </Card>
  );
}

type Filter = "all" | "receipt" | "payment" | "cancelled";

function VoucherList({ vouchers }: { vouchers: VoucherSummary[] }) {
  const navigate = useNavigate();
  const [filter, setFilter] = useState<Filter>("all");
  const [q, setQ] = useState("");
  const match = (v: VoucherSummary) =>
    filter === "all" ? true : filter === "cancelled" ? v.status === "cancelled" : v.voucherType === filter && v.status === "posted";
  const shown = useMemo(() => {
    const n = q.trim().toLowerCase();
    return vouchers.filter((v) => match(v) && (!n || [v.number, v.counterparty, v.reference, v.moneyAccountName].some((x) => x?.toLowerCase().includes(n))));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [vouchers, filter, q]);

  if (vouchers.length === 0) {
    return (
      <EmptyState
        icon={<ArrowDownLeft size={22} />}
        title="No receipts or payments yet"
        body="Record money a customer paid you or a bill you paid. Settle it against invoices or bills, or keep it as an advance."
        actions={
          <Link to="/banking/vouchers/new?type=receipt">
            <Button variant="primary">Record receipt</Button>
          </Link>
        }
      />
    );
  }
  return (
    <>
      <div className="mb-3 flex items-center justify-between gap-4">
        <Segmented<Filter>
          label="Type"
          value={filter}
          onChange={setFilter}
          options={[
            { value: "all", label: "All" },
            { value: "receipt", label: "Receipts" },
            { value: "payment", label: "Payments" },
            { value: "cancelled", label: "Cancelled" },
          ]}
        />
        <SearchField value={q} onChange={setQ} placeholder="Search number, party, UTR" />
      </div>
      <Card className="overflow-hidden">
        <table className="w-full text-[13px]">
          <thead>
            <tr className="border-b border-line bg-surface-2 text-left text-[12px] text-muted">
              <th className="px-4 py-2 font-medium">Date</th>
              <th className="px-4 py-2 font-medium">Number</th>
              <th className="px-4 py-2 font-medium">From / to</th>
              <th className="px-4 py-2 font-medium">Through</th>
              <th className="px-4 py-2 font-medium">Reference</th>
              <th className="px-4 py-2 text-right font-medium">Amount</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {shown.map((v) => {
              const open = () => navigate(`/banking/vouchers/${v.id}`);
              const cancelled = v.status === "cancelled";
              return (
                <tr key={v.id} tabIndex={0} onClick={open} onKeyDown={(e) => e.key === "Enter" && open()} className={clsx("cursor-pointer hover:bg-hover focus-visible:bg-hover focus-visible:outline-none", cancelled && "text-muted")}>
                  <td className="num px-4 py-2.5 text-muted">{shortDate(v.voucherDate)}</td>
                  <td className="px-4 py-2.5">
                    <span className={clsx("num", cancelled && "line-through")}>{v.number}</span>
                    {cancelled && <span className="ml-2"><Badge>Cancelled</Badge></span>}
                    {v.purpose === "refund" && <span className="ml-2"><Badge tone="accent">Refund</Badge></span>}
                  </td>
                  <td className="px-4 py-2.5">{v.counterparty}</td>
                  <td className="px-4 py-2.5 text-muted">
                    {v.moneyAccountName} · {MODE_LABEL[v.mode]}
                  </td>
                  <td className="num px-4 py-2.5 text-muted">{v.reference ?? "—"}</td>
                  <td className={clsx("num px-4 py-2.5 text-right font-medium", !cancelled && v.voucherType === "receipt" && "text-success")}>
                    <span className="sr-only">{VOUCHER_LABEL[v.voucherType]} </span>
                    {v.voucherType === "receipt" ? "+" : "−"}
                    {formatPaise(v.amountPaise)}
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
  );
}
