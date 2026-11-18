import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import clsx from "clsx";
import { Package, Plus, Upload } from "lucide-react";
import { api, keys, type Item } from "../../lib/api";
import { formatPaise, formatRate } from "../../lib/format";
import { Badge, Button, Card, EmptyState, PageHeader, Segmented } from "../../components/ui";
import { SearchField } from "../../components/SearchField";
import { ImportDialog } from "../../components/ImportDialog";
import { ItemDialog, TAX_CATEGORIES } from "./ItemDialog";

type Filter = "all" | "goods" | "service";

export function Items() {
  const { data: profile } = useQuery({ queryKey: keys.profile, queryFn: api.businessProfile });
  const { data: items = [], isLoading } = useQuery({ queryKey: keys.items, queryFn: api.listItems });
  const [filter, setFilter] = useState<Filter>("all");
  const [q, setQ] = useState("");
  const [editing, setEditing] = useState<Item | "new" | null>(null);
  const [importing, setImporting] = useState(false);

  const shown = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return items.filter(
      (i) =>
        (filter === "all" || i.kind === filter) &&
        (!needle || [i.name, i.code, i.hsnSac, i.description].some((v) => v?.toLowerCase().includes(needle))),
    );
  }, [items, filter, q]);

  const ready = !!profile?.organisation;
  const actions = ready && (
    <>
      <Button onClick={() => setImporting(true)}>
        <Upload size={15} /> Import
      </Button>
      <Button variant="primary" onClick={() => setEditing("new")}>
        <Plus size={15} /> New item
      </Button>
    </>
  );

  return (
    <div>
      <PageHeader title="Items" subtitle="Goods and services with HSN/SAC codes, units and GST rates." actions={items.length === 0 ? undefined : actions} />

      {!ready && profile ? (
        <EmptyState
          icon={<Package size={22} />}
          title="Set up your business first"
          body="Items belong to your business, so add its details before your products and services."
          actions={
            <Link to="/setup">
              <Button variant="primary">Set up business</Button>
            </Link>
          }
        />
      ) : !isLoading && items.length === 0 ? (
        <EmptyState
          icon={<Package size={22} />}
          title="Add what you sell and buy"
          body="Each item carries its HSN or SAC code and GST rate, so invoices work out tax for you."
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
                { value: "all", label: "All", count: items.length },
                { value: "goods", label: "Goods", count: items.filter((i) => i.kind === "goods").length },
                { value: "service", label: "Services", count: items.filter((i) => i.kind === "service").length },
              ]}
            />
            <SearchField value={q} onChange={setQ} placeholder="Search name, code, HSN/SAC" />
          </div>
          <Card className="overflow-hidden">
            <table className="w-full text-[13px]">
              <thead>
                <tr className="border-b border-line bg-surface-2 text-left text-[12px] text-muted">
                  <th className="px-4 py-2 font-medium">Name</th>
                  <th className="px-4 py-2 font-medium">HSN/SAC</th>
                  <th className="px-4 py-2 font-medium">Unit</th>
                  <th className="px-4 py-2 font-medium">GST</th>
                  <th className="px-4 py-2 text-right font-medium">Sales price</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-line">
                {shown.map((i) => (
                  <tr
                    key={i.id}
                    tabIndex={0}
                    onClick={() => setEditing(i)}
                    onKeyDown={(e) => e.key === "Enter" && setEditing(i)}
                    className={clsx("cursor-pointer hover:bg-hover focus-visible:bg-hover focus-visible:outline-none", !i.isActive && "text-muted")}
                  >
                    <td className="px-4 py-2.5">
                      <div className="flex items-center gap-2">
                        <span className="font-medium">{i.name}</span>
                        {i.kind === "service" && filter === "all" && <Badge>Service</Badge>}
                        {!i.isActive && <Badge tone="warning">Inactive</Badge>}
                      </div>
                      {i.code && <div className="num text-[12px] text-muted">{i.code}</div>}
                    </td>
                    <td className="num px-4 py-2.5">{i.hsnSac ?? <span className="text-subtle">—</span>}</td>
                    <td className="num px-4 py-2.5 text-muted">{i.unit}</td>
                    <td className="px-4 py-2.5">
                      {i.taxCategory === "taxable" ? (
                        <span className="num font-medium">{formatRate(i.gstRateBp)}</span>
                      ) : (
                        <Badge>{TAX_CATEGORIES.find((c) => c.value === i.taxCategory)?.label}</Badge>
                      )}
                    </td>
                    <td className="num px-4 py-2.5 text-right">
                      {i.salesPricePaise != null ? formatPaise(i.salesPricePaise) : <span className="text-subtle">—</span>}
                      {i.salesPricePaise != null && i.priceIncludesTax && <span className="ml-1 text-[11.5px] text-muted">incl.</span>}
                    </td>
                  </tr>
                ))}
                {shown.length === 0 && (
                  <tr>
                    <td colSpan={5} className="px-4 py-10 text-center text-muted">
                      No items match “{q}”.
                    </td>
                  </tr>
                )}
              </tbody>
            </table>
          </Card>
        </>
      )}

      {editing && <ItemDialog item={editing === "new" ? undefined : editing} onClose={() => setEditing(null)} />}
      {importing && <ImportDialog kind="items" onClose={() => setImporting(false)} />}
    </div>
  );
}
