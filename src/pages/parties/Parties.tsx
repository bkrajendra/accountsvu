import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import clsx from "clsx";
import { Plus, Upload, Users } from "lucide-react";
import { api, keys, type Party } from "../../lib/api";
import { stateName } from "../../lib/gst";
import { formatPaise } from "../../lib/format";
import { Badge, Button, Card, EmptyState, PageHeader, Segmented } from "../../components/ui";
import { SearchField } from "../../components/SearchField";
import { ImportDialog } from "../../components/ImportDialog";
import { GST_TYPES, PartyDialog } from "./PartyDialog";

type Filter = "all" | "customers" | "suppliers";

export function Parties() {
  const { data: profile } = useQuery({ queryKey: keys.profile, queryFn: api.businessProfile });
  const { data: parties = [], isLoading } = useQuery({ queryKey: keys.parties, queryFn: api.listParties });
  const [filter, setFilter] = useState<Filter>("all");
  const [q, setQ] = useState("");
  const [editing, setEditing] = useState<Party | "new" | null>(null);
  const [importing, setImporting] = useState(false);

  const counts = useMemo(
    () => ({
      all: parties.length,
      customers: parties.filter((p) => p.kind !== "supplier").length,
      suppliers: parties.filter((p) => p.kind !== "customer").length,
    }),
    [parties],
  );

  const shown = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return parties.filter((p) => {
      if (filter === "customers" && p.kind === "supplier") return false;
      if (filter === "suppliers" && p.kind === "customer") return false;
      if (!needle) return true;
      return [p.name, p.legalName, p.gstin, p.city, p.phone, p.email].some((v) => v?.toLowerCase().includes(needle));
    });
  }, [parties, filter, q]);

  const ready = !!profile?.organisation;
  const actions = ready && (
    <>
      <Button onClick={() => setImporting(true)}>
        <Upload size={15} /> Import
      </Button>
      <Button variant="primary" onClick={() => setEditing("new")}>
        <Plus size={15} /> New party
      </Button>
    </>
  );

  return (
    <div>
      <PageHeader title="Parties" subtitle="Customers you sell to and suppliers you buy from." actions={parties.length === 0 ? undefined : actions} />

      {!ready && profile ? (
        <EmptyState
          icon={<Users size={22} />}
          title="Set up your business first"
          body="Parties belong to your business, so add its details and GSTIN before adding customers and suppliers."
          actions={
            <Link to="/setup">
              <Button variant="primary">Set up business</Button>
            </Link>
          }
        />
      ) : !isLoading && parties.length === 0 ? (
        <EmptyState
          icon={<Users size={22} />}
          title="Add your customers and suppliers"
          body="Add them one by one, or bring your existing list in from Excel. Opening balances go straight into the ledger."
          actions={actions}
        />
      ) : (
        <>
          <div className="mb-4 flex items-center justify-between gap-4">
            <Segmented<Filter>
              label="Show"
              value={filter}
              onChange={setFilter}
              options={[
                { value: "all", label: "All", count: counts.all },
                { value: "customers", label: "Customers", count: counts.customers },
                { value: "suppliers", label: "Suppliers", count: counts.suppliers },
              ]}
            />
            <SearchField value={q} onChange={setQ} placeholder="Search name, GSTIN, city, phone" />
          </div>
          <Card className="overflow-hidden">
            <table className="w-full text-[13px]">
              <thead>
                <tr className="border-b border-line bg-surface-2 text-left text-[12px] text-muted">
                  <th className="px-4 py-2 font-medium">Name</th>
                  <th className="px-4 py-2 font-medium">GSTIN</th>
                  <th className="px-4 py-2 font-medium">State</th>
                  <th className="px-4 py-2 font-medium">Phone</th>
                  <th className="px-4 py-2 text-right font-medium">Balance</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-line">
                {shown.map((p) => (
                  <tr
                    key={p.id}
                    tabIndex={0}
                    onClick={() => setEditing(p)}
                    onKeyDown={(e) => e.key === "Enter" && setEditing(p)}
                    className={clsx("cursor-pointer hover:bg-hover focus-visible:bg-hover focus-visible:outline-none", !p.isActive && "text-muted")}
                  >
                    <td className="px-4 py-2.5">
                      <div className="flex items-center gap-2">
                        <span className="font-medium">{p.name}</span>
                        {p.kind === "both" && <Badge>Both</Badge>}
                        {p.kind === "supplier" && filter === "all" && <Badge>Supplier</Badge>}
                        {!p.isActive && <Badge tone="warning">Inactive</Badge>}
                      </div>
                      {p.legalName && <div className="text-[12px] text-muted">{p.legalName}</div>}
                    </td>
                    <td className="px-4 py-2.5">
                      {p.gstin ? (
                        <span className="num tracking-[0.02em]">{p.gstin}</span>
                      ) : (
                        <span className="text-muted">{GST_TYPES.find((g) => g.value === p.gstType)?.label}</span>
                      )}
                    </td>
                    <td className="px-4 py-2.5 text-muted">{p.stateCode === 96 ? "Outside India" : stateName(p.stateCode)}</td>
                    <td className="num px-4 py-2.5 text-muted">{p.phone ?? "—"}</td>
                    <td className="px-4 py-2.5 text-right">
                      <Balance paise={p.balancePaise} />
                    </td>
                  </tr>
                ))}
                {shown.length === 0 && (
                  <tr>
                    <td colSpan={5} className="px-4 py-10 text-center text-muted">
                      No parties match “{q}”.
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </Card>
        </>
      )}

      {editing && <PartyDialog party={editing === "new" ? undefined : editing} onClose={() => setEditing(null)} />}
      {importing && <ImportDialog kind="parties" onClose={() => setImporting(false)} />}
    </div>
  );
}

/** Ledger balance in words rather than Dr/Cr: "to receive" / "to pay". */
function Balance({ paise }: { paise: number }) {
  if (paise === 0) return <span className="text-subtle">—</span>;
  return (
    <span>
      <span className="num font-medium">{formatPaise(Math.abs(paise))}</span>
      <span className={clsx("ml-1.5 text-[12px]", paise > 0 ? "text-success" : "text-warning")}>{paise > 0 ? "to receive" : "to pay"}</span>
    </span>
  );
}
