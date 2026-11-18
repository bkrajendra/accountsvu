import { useState } from "react";
import { Banknote, Landmark, Loader2 } from "lucide-react";
import { api, keys, type MoneyAccount, type MoneyAccountInput } from "../../lib/api";
import { paiseToInput } from "../../lib/format";
import { useSave } from "../../lib/mutations";
import { Button } from "../../components/ui";
import { Dialog } from "../../components/Dialog";
import { ChoiceCards, Field, FormGrid, Select, TextInput, Toggle } from "../../components/form";

/** Add or edit a cash drawer or bank account, with its opening balance. */
export function MoneyAccountDialog({ account, onClose }: { account?: MoneyAccount; onClose: () => void }) {
  const [f, setF] = useState<MoneyAccountInput>(() =>
    account
      ? {
          kind: account.kind,
          name: account.name,
          bankName: account.bankName ?? "",
          accountNumber: account.accountNumber ?? "",
          ifsc: account.ifsc ?? "",
          openingBalance: account.openingBalancePaise ? paiseToInput(Math.abs(account.openingBalancePaise)) : "",
          openingBalanceSide: account.openingBalancePaise < 0 ? "credit" : "debit",
          isActive: account.isActive,
        }
      : { kind: "bank", name: "", bankName: "", accountNumber: "", ifsc: "", openingBalance: "", openingBalanceSide: "debit" },
  );
  const save = useSave(
    (x: MoneyAccountInput) => (account ? api.updateMoneyAccount(account.id, x, account.rowVersion) : api.createMoneyAccount(x)),
    {
      invalidate: [keys.moneyAccounts, keys.accounts, keys.openingBalances, keys.voucherAccounts, ["moneyBook"]],
      success: account ? "Account updated" : "Account added",
      onSuccess: onClose,
    },
  );
  const err = save.fieldErrors;
  const set = <K extends keyof MoneyAccountInput>(k: K, v: MoneyAccountInput[K]) => {
    setF((x) => ({ ...x, [k]: v }));
    save.clearError(k);
  };
  const bank = f.kind === "bank";

  return (
    <Dialog
      open
      onClose={onClose}
      width={560}
      title={account ? `Edit ${account.name}` : "Add a cash or bank account"}
      description="Each account gets its own ledger, so its book, balance and statements stay separate."
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={save.isPending} onClick={() => save.mutate(f)}>
            {save.isPending && <Loader2 size={15} className="animate-spin" />} {account ? "Save" : "Add account"}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        {!account && (
          <ChoiceCards<MoneyAccountInput["kind"]>
            name="Account type"
            value={f.kind}
            onChange={(k) => set("kind", k)}
            options={[
              { value: "bank", title: "Bank account", description: "Current, savings or overdraft", icon: <Landmark size={18} /> },
              { value: "cash", title: "Cash", description: "A second cash drawer or petty cash", icon: <Banknote size={18} /> },
            ]}
          />
        )}
        <FormGrid>
          <Field label="Name" error={err.name} className="col-span-2" hint={bank ? "How it appears in lists, e.g. HDFC Current." : undefined}>
            {(p) => <TextInput id={p.id} invalid={p.invalid} autoFocus value={f.name} onChange={(e) => set("name", e.target.value)} />}
          </Field>
          {bank && (
            <>
              <Field label="Bank" optional error={err.bankName}>
                {(p) => <TextInput id={p.id} invalid={p.invalid} value={f.bankName ?? ""} onChange={(e) => set("bankName", e.target.value)} />}
              </Field>
              <Field label="IFSC" optional error={err.ifsc}>
                {(p) => <TextInput id={p.id} invalid={p.invalid} className="num uppercase" maxLength={11} value={f.ifsc ?? ""} onChange={(e) => set("ifsc", e.target.value)} />}
              </Field>
              <Field label="Account number" optional error={err.accountNumber} className="col-span-2">
                {(p) => <TextInput id={p.id} invalid={p.invalid} className="num" inputMode="numeric" value={f.accountNumber ?? ""} onChange={(e) => set("accountNumber", e.target.value)} />}
              </Field>
            </>
          )}
          <Field label="Opening balance (₹)" optional error={err.openingBalance} hint="As on the day your books start.">
            {(p) => <TextInput id={p.id} invalid={p.invalid} className="num" inputMode="decimal" placeholder="0.00" value={f.openingBalance ?? ""} onChange={(e) => set("openingBalance", e.target.value)} />}
          </Field>
          <Field label="Balance is" error={err.openingBalanceSide}>
            {(p) => (
              <Select id={p.id} value={f.openingBalanceSide} onChange={(e) => set("openingBalanceSide", e.target.value as "debit" | "credit")}>
                <option value="debit">Money in the account</option>
                {bank && <option value="credit">Overdrawn (owed to bank)</option>}
              </Select>
            )}
          </Field>
        </FormGrid>
        {account && (
          <div className="border-t border-line pt-4">
            <Toggle checked={f.isActive ?? true} onChange={(v) => set("isActive", v)} label="Active" description="Closed accounts keep their history but can't take new entries." />
            {err.isActive && <p className="mt-2 text-[12px] text-danger">{err.isActive}</p>}
          </div>
        )}
      </div>
    </Dialog>
  );
}
