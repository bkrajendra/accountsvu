import { useQuery } from "@tanstack/react-query";
import { api, keys, type Account } from "../../lib/api";
import { Badge, Card } from "../../components/ui";
import { SettingsSection } from "./SettingsSection";

const KINDS: { kind: Account["kind"]; label: string }[] = [
  { kind: "asset", label: "Assets" },
  { kind: "liability", label: "Liabilities" },
  { kind: "equity", label: "Equity" },
  { kind: "income", label: "Income" },
  { kind: "expense", label: "Expenses" },
];

export function AccountsTab() {
  const { data: accounts = [] } = useQuery({ queryKey: keys.accounts, queryFn: api.listAccounts });

  return (
    <SettingsSection
      title="Chart of accounts"
      description="The ledgers your books post to. System accounts are used by invoices, bills and GST; adding and renaming accounts comes with the ledger module."
    >
      {accounts.length === 0 ? (
        <Card className="p-6 text-center text-muted">Accounts are created when you set up your business.</Card>
      ) : (
        <div className="space-y-5">
          {KINDS.map(({ kind, label }) => {
            const rows = accounts.filter((a) => a.kind === kind);
            if (!rows.length) return null;
            return (
              <div key={kind}>
                <h3 className="mb-2 flex items-center gap-2 text-[12px] font-semibold uppercase tracking-[0.04em] text-muted">
                  {label} <span className="font-medium normal-case tracking-normal">· {rows.length}</span>
                </h3>
                <Card className="divide-y divide-line">
                  {rows.map((a) => (
                    <div key={a.id} className="flex items-center gap-4 px-4 py-2.5 text-[13px]">
                      <span className="num w-12 text-muted">{a.code}</span>
                      <span className="flex-1 font-medium">{a.name}</span>
                      {a.systemRole && <Badge>System</Badge>}
                    </div>
                  ))}
                </Card>
              </div>
            );
          })}
        </div>
      )}
    </SettingsSection>
  );
}
