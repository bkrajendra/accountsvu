import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { ArrowDownLeft, ArrowUpRight, Link2 } from "lucide-react";
import { AdjustAdvanceDialog } from "./AdjustAdvanceDialog";
import { api, keys } from "../../lib/api";
import { formatPaise } from "../../lib/format";
import { shortDate } from "../../lib/sales";
import { Button, Card } from "../../components/ui";

/**
 * Receipts or payments that settled an invoice or bill, with what is still
 * outstanding and a shortcut to record the next one.
 */
export function SettlementsCard({
  kind,
  documentId,
  partyId,
  totalPaise,
  paidPaise,
  adjustedPaise,
  open,
}: {
  kind: "sales" | "purchase";
  documentId: string;
  partyId: string | null;
  totalPaise: number;
  paidPaise: number;
  adjustedPaise: number;
  /** Whether more money can be recorded against it. */
  open: boolean;
}) {
  const { data = [], isLoading } = useQuery({ queryKey: keys.settlements(kind, documentId), queryFn: () => api.documentSettlements(kind, documentId) });
  const outstanding = totalPaise - paidPaise - adjustedPaise;
  const type = kind === "sales" ? "receipt" : "payment";
  // Advances of this party that could settle it.
  const canSettle = open && outstanding > 0 && !!partyId;
  const { data: advances = [] } = useQuery({
    queryKey: keys.partyAdvances(partyId ?? "", type),
    queryFn: () => api.partyAdvances(partyId!, type),
    enabled: canSettle,
  });
  const [using, setUsing] = useState<string | null>(null);
  return (
    <Card className="p-4 text-[13px]">
      <div className="mb-2 text-[12px] font-semibold uppercase tracking-[0.04em] text-muted">{kind === "sales" ? "Receipts" : "Payments"}</div>
      {isLoading ? null : data.length === 0 ? (
        <p className="text-muted">{paidPaise > 0 ? "Paid when recorded." : "None yet"}</p>
      ) : (
        data.map((s) => (
          <Link key={s.voucherId} to={`/banking/vouchers/${s.voucherId}`} className="flex justify-between py-1 hover:text-accent">
            <span>
              <span className="num">{s.number}</span>
              <span className="ml-2 text-muted">{shortDate(s.voucherDate)}</span>
            </span>
            <span className="num">{formatPaise(s.amountPaise)}</span>
          </Link>
        ))
      )}
      <div className="mt-2 flex justify-between border-t border-line pt-2 font-medium">
        <span>Outstanding</span>
        <span className="num">{formatPaise(Math.max(outstanding, 0))}</span>
      </div>
      {canSettle &&
        advances.map((a) => (
          <Button key={a.voucherId} className="mt-3 h-auto w-full justify-start py-2 text-left" onClick={() => setUsing(a.voucherId)}>
            <Link2 size={15} className="shrink-0" />
            <span className="min-w-0">
              Use advance
              <span className="num block whitespace-nowrap text-[12px] font-normal text-muted">{a.number}</span>
              <span className="num block text-[12px] font-normal text-muted">{formatPaise(a.onAccountPaise)} left</span>
            </span>
          </Button>
        ))}
      {using && <AdvanceDialog id={using} documentId={documentId} onClose={() => setUsing(null)} />}
      {canSettle && (
        <Link to={`/banking/vouchers/new?type=${type}&party=${partyId}&doc=${documentId}`} className="mt-3 block">
          <Button variant="primary" className="w-full">
            {type === "receipt" ? <ArrowDownLeft size={15} /> : <ArrowUpRight size={15} />} Record {type}
          </Button>
        </Link>
      )}
    </Card>
  );
}

function AdvanceDialog({ id, documentId, onClose }: { id: string; documentId: string; onClose: () => void }) {
  const { data: v } = useQuery({ queryKey: keys.voucher(id), queryFn: () => api.getVoucher(id) });
  return v ? <AdjustAdvanceDialog v={v} documentId={documentId} onClose={onClose} /> : null;
}
