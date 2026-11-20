import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Loader2, Package, Wrench } from "lucide-react";
import { api, keys, type Item, type ItemInput, type ItemKind, type TaxCategory } from "../../lib/api";
import { formatRate, paiseToInput } from "../../lib/format";
import { useSave } from "../../lib/mutations";
import { Button } from "../../components/ui";
import { Dialog } from "../../components/Dialog";
import { ChoiceCards, Field, FormGrid, Select, TextInput, Toggle } from "../../components/form";
import { rateLabel } from "../../components/TaxWarnings";
import { todayIso } from "../../lib/sales";
import { HsnHint } from "./HsnHint";

export const TAX_CATEGORIES: { value: TaxCategory; label: string }[] = [
  { value: "taxable", label: "Taxable" },
  { value: "exempt", label: "Exempt" },
  { value: "nil_rated", label: "Nil-rated" },
  { value: "non_gst", label: "Non-GST" },
];

type Draft = Required<ItemInput>;

/** Basis points → the percent string the core parses ("18", "0.25"). */
function bpToPercent(bp: number): string {
  return formatRate(bp).replace("%", "");
}

function draftFrom(i?: Item): Draft {
  return {
    kind: i?.kind ?? "goods",
    name: i?.name ?? "",
    code: i?.code ?? "",
    description: i?.description ?? "",
    hsnSac: i?.hsnSac ?? "",
    unit: i?.unit ?? "NOS",
    taxCategory: i?.taxCategory ?? "taxable",
    gstRate: i ? bpToPercent(i.gstRateBp) : "18",
    cessRate: i?.cessRateBp ? bpToPercent(i.cessRateBp) : "",
    salesPrice: paiseToInput(i?.salesPricePaise),
    purchasePrice: paiseToInput(i?.purchasePricePaise),
    priceIncludesTax: i?.priceIncludesTax ?? false,
    trackStock: i?.trackStock ?? false,
    isActive: i?.isActive ?? true,
  };
}

export function ItemDialog({ item, onClose }: { item?: Item; onClose: () => void }) {
  const { data: options } = useQuery({ queryKey: keys.itemOptions, queryFn: api.itemOptions, staleTime: Infinity });
  const [d, setD] = useState<Draft>(() => draftFrom(item));
  const save = useSave((input: ItemInput) => (item ? api.updateItem(item.id, input, item.rowVersion) : api.createItem(input)), {
    invalidate: [keys.items],
    success: item ? "Item saved" : "Item added",
    onSuccess: onClose,
  });
  const err = save.fieldErrors;
  const set = <K extends keyof Draft>(k: K, v: Draft[K]) => {
    setD((x) => ({ ...x, [k]: v }));
    save.clearError(k);
  };
  const taxable = d.taxCategory === "taxable";
  const goods = d.kind === "goods";

  function submit() {
    save.mutate({
      ...d,
      gstRate: taxable ? d.gstRate : undefined,
      trackStock: goods && d.trackStock,
    });
  }

  return (
    <Dialog
      open
      onClose={onClose}
      width={620}
      title={item ? item.name : "New item"}
      description={item ? "Edit the item's tax and price details." : "Something you sell or buy: goods with an HSN code, or a service with a SAC code."}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={save.isPending} onClick={submit}>
            {save.isPending && <Loader2 size={15} className="animate-spin" />} {item ? "Save" : "Add item"}
          </Button>
        </>
      }
    >
      <div className="space-y-5">
        <ChoiceCards<ItemKind>
          name="Item type"
          value={d.kind}
          onChange={(v) => {
            set("kind", v);
            if (!item && (d.unit === "NOS" || d.unit === "OTH")) set("unit", v === "service" ? "OTH" : "NOS");
          }}
          options={[
            { value: "goods", title: "Goods", description: "Physical products · HSN code", icon: <Package size={17} /> },
            { value: "service", title: "Service", description: "Work or time · SAC code", icon: <Wrench size={17} /> },
          ]}
        />
        <FormGrid>
          <Field label="Name" error={err.name} className="col-span-2">
            {(p) => <TextInput id={p.id} invalid={p.invalid} autoFocus value={d.name} onChange={(e) => set("name", e.target.value)} />}
          </Field>
          <Field label="Item code" optional error={err.code}>
            {(p) => <TextInput id={p.id} invalid={p.invalid} className="num" value={d.code} onChange={(e) => set("code", e.target.value)} />}
          </Field>
          <Field label={goods ? "HSN code" : "SAC code"} hint={goods ? "4, 6 or 8 digits." : "6 digits, starts with 99."} error={err.hsnSac}>
            {(p) => (
              <TextInput id={p.id} invalid={p.invalid} className="num" inputMode="numeric" maxLength={8} value={d.hsnSac} onChange={(e) => set("hsnSac", e.target.value.replace(/\D/g, ""))} />
            )}
          </Field>
          <HsnHint
            code={d.hsnSac}
            name={d.name}
            taxCategory={d.taxCategory}
            rateBp={Math.round(Number(d.gstRate) * 100)}
            onUse={(s, withCode) => {
              if (withCode) {
                set("hsnSac", s.code);
                set("kind", s.kind);
              }
              set("taxCategory", s.taxCategory);
              if (s.taxCategory === "taxable") set("gstRate", bpToPercent(s.rateBp));
            }}
          />
          <Field label="Description" optional error={err.description} className="col-span-2">
            {(p) => <TextInput id={p.id} invalid={p.invalid} value={d.description} onChange={(e) => set("description", e.target.value)} />}
          </Field>
        </FormGrid>

        <FormGrid>
          <Field label="GST treatment" error={err.taxCategory}>
            {(p) => (
              <Select id={p.id} invalid={p.invalid} value={d.taxCategory} onChange={(e) => set("taxCategory", e.target.value as TaxCategory)}>
                {TAX_CATEGORIES.map((c) => (
                  <option key={c.value} value={c.value}>
                    {c.label}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field label="GST rate" error={err.gstRate}>
            {(p) => (
              <Select id={p.id} invalid={p.invalid} disabled={!taxable} value={taxable ? d.gstRate : "0"} onChange={(e) => set("gstRate", e.target.value)}>
                {!taxable && <option value="0">No GST</option>}
                {options?.gstRates
                  .filter((r) => r.basisPoints > 0)
                  .map((r) => (
                    <option key={r.basisPoints} value={bpToPercent(r.basisPoints)}>
                      {rateLabel(r, todayIso())}
                    </option>
                  ))}
              </Select>
            )}
          </Field>
          <Field label="Unit" hint="GST unit of quantity (UQC)." error={err.unit}>
            {(p) => (
              <Select id={p.id} invalid={p.invalid} value={d.unit} onChange={(e) => set("unit", e.target.value)}>
                {options?.units.map((u) => (
                  <option key={u.code} value={u.code}>
                    {u.code} — {u.label}
                  </option>
                ))}
              </Select>
            )}
          </Field>
          <Field label="Cess rate (%)" optional error={err.cessRate}>
            {(p) => <TextInput id={p.id} invalid={p.invalid} className="num" inputMode="decimal" value={d.cessRate} onChange={(e) => set("cessRate", e.target.value)} />}
          </Field>
        </FormGrid>

        <FormGrid>
          <Field label="Sales price (₹)" optional error={err.salesPrice}>
            {(p) => <TextInput id={p.id} invalid={p.invalid} className="num" inputMode="decimal" value={d.salesPrice} onChange={(e) => set("salesPrice", e.target.value)} />}
          </Field>
          <Field label="Purchase price (₹)" optional error={err.purchasePrice}>
            {(p) => <TextInput id={p.id} invalid={p.invalid} className="num" inputMode="decimal" value={d.purchasePrice} onChange={(e) => set("purchasePrice", e.target.value)} />}
          </Field>
        </FormGrid>

        <div className="space-y-4">
          <Toggle checked={d.priceIncludesTax} onChange={(v) => set("priceIncludesTax", v)} label="Prices include GST" description="The tax is worked out of the price instead of added on top." />
          {goods && <Toggle checked={d.trackStock} onChange={(v) => set("trackStock", v)} label="Track stock" description="Stock quantities arrive in a later release; this marks the item for it." />}
          {item && <Toggle checked={d.isActive} onChange={(v) => set("isActive", v)} label="Active" description="Inactive items aren't offered on new documents." />}
        </div>
      </div>
    </Dialog>
  );
}
