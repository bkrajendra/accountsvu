import { useQuery } from "@tanstack/react-query";
import { ArrowLeftRight, BadgeCheck, Building2, Landmark, Scale, FileDigit, History, ListTree, Package, Paperclip, Receipt, ShoppingBag, Upload, Users, type LucideIcon } from "lucide-react";
import { api, keys, type AuditEvent } from "../../lib/api";
import { Card } from "../../components/ui";
import { documentTypeLabel } from "../../lib/gst";
import { SettingsSection } from "./SettingsSection";

const ENTITY: Record<string, { label: string; icon: LucideIcon }> = {
  organisation: { label: "Business details", icon: Building2 },
  gst_registration: { label: "GST registration", icon: BadgeCheck },
  invoice_series: { label: "Number series", icon: FileDigit },
  chart_of_accounts: { label: "Chart of accounts", icon: ListTree },
  party: { label: "Party", icon: Users },
  item: { label: "Item", icon: Package },
  import: { label: "Spreadsheet import", icon: Upload },
  sales_document: { label: "Sales document", icon: Receipt },
  purchase_document: { label: "Purchase", icon: ShoppingBag },
  purchase_line: { label: "Input tax credit", icon: ShoppingBag },
  attachment: { label: "Attachment", icon: Paperclip },
  money_account: { label: "Cash or bank account", icon: Landmark },
  money_voucher: { label: "Receipt or payment", icon: ArrowLeftRight },
  bank_statement: { label: "Bank statement", icon: Upload },
  bank_statement_line: { label: "Statement line", icon: Landmark },
  account: { label: "Ledger account", icon: Scale },
};

const VERB: Record<string, string> = { create: "created", update: "updated", seed: "created", run: "completed", issue: "issued", cancel: "cancelled", delete: "deleted", record: "recorded", itc_review: "reviewed", receipt: "recorded", payment: "recorded", match: "matched", unmatch: "unmatched", ignore: "ignored", import: "imported", undo_import: "import undone", opening_balance: "opening balance set" };

/** Field names that changed between the before/after snapshots. */
function changedFields(e: AuditEvent): string[] {
  if (!e.beforeJson || !e.afterJson) return [];
  try {
    const a = JSON.parse(e.beforeJson) as Record<string, unknown>;
    const b = JSON.parse(e.afterJson) as Record<string, unknown>;
    const ignore = new Set(["row_version", "updated_at", "rowVersion", "updatedAt"]);
    return Object.keys(b)
      .filter((k) => !ignore.has(k) && JSON.stringify(a[k]) !== JSON.stringify(b[k]))
      .map((k) => k.replace(/_/g, " ").replace(/([a-z])([A-Z])/g, "$1 $2").toLowerCase());
  } catch {
    return [];
  }
}

/** A short identifier for the record, e.g. a GSTIN or a series prefix. */
function subject(e: AuditEvent): string | null {
  try {
    const a = JSON.parse(e.afterJson ?? e.beforeJson ?? "null") as Record<string, unknown> | null;
    if (!a) return null;
    if (e.entityType === "invoice_series") return `${documentTypeLabel(String(a.documentType))} · ${a.prefix}`;
    if (e.entityType === "gst_registration") return String(a.gstin);
    if (e.entityType === "organisation") return String(a.tradeName ?? a.legalName);
    if (e.entityType === "chart_of_accounts") return `${a.accounts} accounts`;
    if (e.entityType === "party" || e.entityType === "item") return String(a.name);
    if (e.entityType === "sales_document") return `${a.number ?? "Draft"} · ${a.partyName}`;
    if (e.entityType === "purchase_document") return `${a.number ?? a.supplierInvoiceNumber ?? "Expense"} · ${a.partyName ?? "no supplier"}`;
    if (e.entityType === "purchase_line") return `${a.description} · ${a.itcStatus}`;
    if (e.entityType === "attachment") return `${a.fileName}`;
    if (e.entityType === "money_account") return String(a.name);
    if (e.entityType === "money_voucher") return String(a.number);
    if (e.entityType === "bank_statement") return a.file ? `${a.file} · ${a.imported} lines added` : `${a.removed} lines removed`;
    if (e.entityType === "bank_statement_line") return String(a.description);
    if (e.entityType === "import") return `${a.fileName} · ${a.created} ${a.kind} added${a.skipped ? `, ${a.skipped} skipped` : ""}`;
  } catch {
    /* older or partial snapshot */
  }
  return null;
}

export function ActivityTab() {
  const { data: events = [] } = useQuery({ queryKey: keys.audit, queryFn: () => api.listAuditEvents(200) });

  return (
    <SettingsSection
      title="Activity log"
      description="Every change to settings and masters is recorded with who made it and when. Entries can't be edited or deleted."
    >
      {events.length === 0 ? (
        <Card className="p-6 text-center text-muted">No activity yet.</Card>
      ) : (
        <Card className="divide-y divide-line">
          {events.map((e) => {
            const [entity, verb] = e.action.split(".");
            const meta = ENTITY[entity] ?? { label: entity.replace(/_/g, " "), icon: History };
            const changed = changedFields(e);
            const what = subject(e);
            const Icon = meta.icon;
            return (
              <div key={e.id} className="flex items-start gap-3.5 px-4 py-3">
                <span className="mt-0.5 grid size-8 shrink-0 place-items-center rounded-lg bg-active text-muted">
                  <Icon size={15} />
                </span>
                <div className="min-w-0 flex-1">
                  <div className="text-[13.5px]">
                    <span className="font-medium">{meta.label}</span> {VERB[verb] ?? verb}
                    {what && <span className="num ml-1.5 text-muted">· {what}</span>}
                  </div>
                  {changed.length > 0 && <div className="mt-0.5 truncate text-[12.5px] text-muted">Changed: {changed.join(", ")}</div>}
                </div>
                <time className="num shrink-0 text-[12px] text-muted" dateTime={e.occurredAt}>
                  {new Date(e.occurredAt).toLocaleString("en-IN", { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" })}
                </time>
              </div>
            );
          })}
        </Card>
      )}
    </SettingsSection>
  );
}
