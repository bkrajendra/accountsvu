import { useEffect, useState } from "react";
import { AlertTriangle, CheckCircle2, FileArchive, Loader2 } from "lucide-react";
import { api, ApiError, type ChosenBackup, type RestoreCheck } from "../../lib/api";
import { shortDate } from "../../lib/sales";
import { useToast } from "../../components/Toast";
import { Button } from "../../components/ui";
import { Dialog } from "../../components/Dialog";
import { Field, TextInput } from "../../components/form";

const stamp = (iso: string) =>
  new Intl.DateTimeFormat("en-IN", { day: "numeric", month: "short", year: "numeric", hour: "numeric", minute: "2-digit" }).format(new Date(iso));

const size = (b: number) => (b >= 1_048_576 ? `${(b / 1_048_576).toFixed(1)} MB` : `${Math.max(1, Math.round(b / 1024))} KB`);

/**
 * Restore in three steps: choose the file (its readable header is shown),
 * unlock and check it with the passphrase, then replace the data and restart.
 * Nothing changes until the last step.
 */
export function RestoreDialog({ onClose, freshInstall = false }: { onClose: () => void; freshInstall?: boolean }) {
  const toast = useToast();
  const [chosen, setChosen] = useState<ChosenBackup | null>(null);
  const [pass, setPass] = useState("");
  const [check, setCheck] = useState<RestoreCheck | null>(null);
  const [busy, setBusy] = useState<null | "choose" | "check" | "apply">(null);
  const [errors, setErrors] = useState<{ passphrase?: string; form?: string }>({});

  // Whatever happens, a checked-but-unused copy is discarded on close.
  useEffect(() => () => void api.cancelRestore().catch(() => {}), []);

  const run = async (kind: "choose" | "check" | "apply", f: () => Promise<void>) => {
    setBusy(kind);
    setErrors({});
    try {
      await f();
    } catch (e) {
      if (e instanceof ApiError && e.fields.passphrase) setErrors({ passphrase: e.fields.passphrase });
      else setErrors({ form: e instanceof Error ? e.message : String(e) });
    } finally {
      setBusy(null);
    }
  };

  const choose = () =>
    run("choose", async () => {
      const c = await api.chooseBackupFile();
      if (c) {
        setChosen(c);
        setCheck(null);
        setPass("");
      }
    });
  const verify = () => run("check", async () => setCheck(await api.verifyBackup(pass)));
  const apply = () =>
    run("apply", async () => {
      await api.applyRestore();
      toast("Restoring… AccountsVU is restarting");
    });

  const c = check?.check;
  return (
    <Dialog
      open
      onClose={onClose}
      width={540}
      title="Restore from a backup"
      description={freshInstall ? "Bring your books onto this computer from an AccountsVU backup file." : "Replace the data on this computer with a backup."}
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          {!check ? (
            <Button variant="primary" disabled={!chosen || !pass || !!busy} onClick={verify}>
              {busy === "check" && <Loader2 size={15} className="animate-spin" />} Check backup
            </Button>
          ) : (
            <Button variant="primary" className={freshInstall ? undefined : "bg-danger hover:bg-danger"} disabled={!!busy} onClick={apply}>
              {busy === "apply" && <Loader2 size={15} className="animate-spin" />} {freshInstall ? "Restore and restart" : "Replace data and restart"}
            </Button>
          )}
        </>
      }
    >
      <div className="space-y-4 text-[13px]">
        <div className="flex items-center gap-3 rounded-xl border border-line p-3">
          <span className="grid size-9 shrink-0 place-items-center rounded-lg bg-accent-soft text-accent">
            <FileArchive size={17} />
          </span>
          <div className="min-w-0 flex-1">
            {chosen ? (
              <>
                <div className="truncate font-medium">{chosen.header.businessName}</div>
                <div className="truncate text-[12.5px] text-muted">
                  Made {stamp(chosen.header.createdAt)} · {size(chosen.sizeBytes)} · {chosen.fileName}
                </div>
              </>
            ) : (
              <div className="text-muted">No file chosen</div>
            )}
          </div>
          <Button disabled={!!busy} onClick={choose}>
            {busy === "choose" && <Loader2 size={15} className="animate-spin" />} {chosen ? "Change…" : "Choose file…"}
          </Button>
        </div>

        {chosen && !check && (
          <Field label="Backup passphrase" error={errors.passphrase}>
            {(p) => (
              <TextInput
                id={p.id}
                invalid={p.invalid}
                type="password"
                autoFocus
                value={pass}
                onChange={(e) => setPass(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && pass && !busy && void verify()}
              />
            )}
          </Field>
        )}

        {c && (
          <>
            <div className="rounded-xl bg-success-soft p-3">
              <div className="flex items-center gap-2 font-semibold text-success">
                <CheckCircle2 size={15} /> Backup opened and checked
              </div>
              <ul className="mt-2 grid grid-cols-2 gap-x-6 gap-y-1 text-[12.5px] text-fg">
                <li>Integrity: {c.integrity === "ok" ? "OK" : c.integrity}</li>
                <li>Every entry balances: {c.unbalancedEntries === 0 ? "Yes" : `No (${c.unbalancedEntries})`}</li>
                <li>{c.salesDocuments} sales documents</li>
                <li>{c.purchaseDocuments} purchases</li>
                <li>{c.vouchers} receipts & payments</li>
                <li>{c.parties} parties · {c.items} items</li>
                <li>{c.journalEntries} journal entries</li>
                <li>Last entry {c.lastEntryDate ? shortDate(c.lastEntryDate) : "—"}</li>
              </ul>
            </div>
            {!freshInstall && (
              <p className="flex gap-2 rounded-xl bg-warning-soft px-3 py-2.5 text-[12.5px] text-fg">
                <AlertTriangle size={15} className="mt-px shrink-0 text-warning" aria-hidden />
                Everything now in AccountsVU on this computer is replaced by this backup. A copy of the current data is kept in the app's data folder. The app restarts.
              </p>
            )}
          </>
        )}

        {errors.form && (
          <p className="rounded-lg bg-danger-soft px-3 py-2 text-[12.5px] text-danger" role="alert">
            {errors.form}
          </p>
        )}
      </div>
    </Dialog>
  );
}
