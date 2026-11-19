import { useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AlertTriangle, HardDriveDownload, HardDriveUpload, KeyRound, Loader2, Lock, LockOpen, ShieldCheck } from "lucide-react";
import { api, IDLE_CHOICES, keys, runningInTauri } from "../../lib/api";
import { useSave } from "../../lib/mutations";
import { useSession } from "../../lib/session";
import { useToast } from "../../components/Toast";
import { Badge, Button, Card } from "../../components/ui";
import { Dialog } from "../../components/Dialog";
import { Field, Select, TextInput } from "../../components/form";
import { SettingsSection } from "./SettingsSection";
import { RestoreDialog } from "./RestoreDialog";

export function localStamp(iso: string): string {
  return new Intl.DateTimeFormat("en-IN", { day: "numeric", month: "short", year: "numeric", hour: "numeric", minute: "2-digit" }).format(new Date(iso));
}

function ago(days: number | null): string {
  if (days === null) return "never";
  if (days === 0) return "today";
  if (days === 1) return "yesterday";
  return `${days} days ago`;
}

export const idleLabel = (m: number) => (m === 0 ? "Only when the app starts" : m === 60 ? "After 1 hour idle" : `After ${m} minute${m === 1 ? "" : "s"} idle`);

/** Backups, restore and the app lock (Phase 1.7). */
export function SecurityTab() {
  const { data: status } = useQuery({ queryKey: keys.backupStatus, queryFn: api.backupStatus });
  const { lock, setLock, lockNow } = useSession();
  const [dialog, setDialog] = useState<null | "backup" | "restore" | "set-lock" | "change-lock" | "remove-lock">(null);
  const close = () => setDialog(null);
  const last = status?.lastBackup;

  return (
    <>
      <SettingsSection
        title="Backups"
        description="A backup is one encrypted file with everything: books, documents, attachments and settings. Keep copies off this computer, for example on a USB drive or in a cloud folder."
      >
        <Card className="flex items-center gap-4 p-5">
          <span className={status?.due ? "grid size-10 shrink-0 place-items-center rounded-xl bg-warning-soft text-warning" : "grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent"}>
            <HardDriveDownload size={18} />
          </span>
          <div className="min-w-0 flex-1 text-[13px]">
            <div className="flex items-center gap-2 font-semibold">
              {last ? `Last backup ${ago(status?.daysSinceBackup ?? null)}` : "No backup yet"}
              {status?.due && <Badge tone="warning" icon={<AlertTriangle size={11} />}>{last ? "Due" : "Recommended"}</Badge>}
            </div>
            <div className="truncate text-muted">{last ? `${localStamp(last.happenedAt)} · ${last.fileName}` : "Back up at least once a week, and before big changes."}</div>
          </div>
          <Button onClick={() => setDialog("restore")}>
            <HardDriveUpload size={15} /> Restore…
          </Button>
          <Button variant="primary" onClick={() => setDialog("backup")}>
            Back up now
          </Button>
        </Card>
        {status?.lastRestore && (
          <p className="mt-3 text-[12.5px] text-muted">
            Restored from a backup on {localStamp(status.lastRestore.happenedAt)}
            {status.lastRestore.fileName && `; the data it replaced was kept as ${status.lastRestore.fileName}`}.
          </p>
        )}
      </SettingsSection>

      <SettingsSection
        title="App lock"
        description="Ask for a PIN or password when AccountsVU opens and after it has been idle. It stops someone using an unattended computer; for protection if the computer is lost, also turn on BitLocker."
      >
        <Card className="divide-y divide-line">
          <div className="flex items-center gap-4 p-5">
            <span className="grid size-10 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">{lock.enabled ? <Lock size={18} /> : <LockOpen size={18} />}</span>
            <div className="min-w-0 flex-1 text-[13px]">
              <div className="font-semibold">{lock.enabled ? "On" : "Off"}</div>
              <div className="text-muted">{lock.enabled ? idleLabel(lock.idleMinutes) : "Anyone at this computer can open your books."}</div>
            </div>
            {lock.enabled ? (
              <>
                <Button variant="ghost" onClick={() => setDialog("remove-lock")}>Turn off</Button>
                <Button onClick={() => setDialog("change-lock")}>Change PIN</Button>
                <Button variant="primary" onClick={() => void lockNow()}>
                  <Lock size={15} /> Lock now
                </Button>
              </>
            ) : (
              <Button variant="primary" onClick={() => setDialog("set-lock")}>
                <KeyRound size={15} /> Turn on
              </Button>
            )}
          </div>
          {lock.enabled && <TimeoutRow minutes={lock.idleMinutes} onSaved={setLock} />}
        </Card>
      </SettingsSection>

      <SettingsSection title="Desktop security" description="How AccountsVU keeps your data on this computer.">
        <Card className="space-y-2.5 p-5 text-[13px]">
          {[
            "Your books stay in one database file on this computer. Nothing is sent anywhere.",
            "The app window can only use AccountsVU's own commands. It has no file, shell or network access of its own, and files open only through system dialogs.",
            "Backups use AES-256-GCM with a key derived from your passphrase (Argon2id). A changed or damaged file is refused.",
            "A restore is checked first: file integrity, links between records, and that every journal entry balances.",
          ].map((t) => (
            <p key={t} className="flex gap-2.5">
              <ShieldCheck size={15} className="mt-0.5 shrink-0 text-success" aria-hidden />
              <span className="text-muted">{t}</span>
            </p>
          ))}
          {!runningInTauri && <p className="pt-1 text-[12px] text-subtle">Browser preview: files, PDFs and restarts are simulated.</p>}
        </Card>
      </SettingsSection>

      {dialog === "backup" && <BackupDialog onClose={close} />}
      {dialog === "restore" && <RestoreDialog onClose={close} />}
      {(dialog === "set-lock" || dialog === "change-lock") && <SetLockDialog change={dialog === "change-lock"} idle={lock.idleMinutes || 5} onClose={close} />}
      {dialog === "remove-lock" && <RemoveLockDialog onClose={close} />}
    </>
  );
}

function TimeoutRow({ minutes, onSaved }: { minutes: number; onSaved: (s: Awaited<ReturnType<typeof api.setLockTimeout>>) => void }) {
  const save = useSave((m: number) => api.setLockTimeout(m), { invalidate: [], success: "Lock timing updated", onSuccess: onSaved });
  return (
    <div className="flex items-center gap-4 px-5 py-3 text-[13px]">
      <label htmlFor="lock-timeout" className="flex-1 text-muted">Lock automatically</label>
      <Select id="lock-timeout" className="w-[230px]" value={minutes} disabled={save.isPending} onChange={(e) => save.mutate(Number(e.target.value))}>
        {IDLE_CHOICES.map((m) => (
          <option key={m} value={m}>{idleLabel(m)}</option>
        ))}
      </Select>
    </div>
  );
}

function BackupDialog({ onClose }: { onClose: () => void }) {
  const qc = useQueryClient();
  const toast = useToast();
  const [pass, setPass] = useState("");
  const [again, setAgain] = useState("");
  const mismatch = again.length > 0 && pass !== again;
  const save = useSave((p: string) => api.createBackup(p), {
    invalidate: [keys.backupStatus],
    success: "",
    onSuccess: (r) => {
      if (!r) return; // the save dialog was cancelled
      qc.setQueryData(keys.backupStatus, r.status);
      toast(`Backup saved: ${r.file.fileName}`, "success", { label: "Show in folder", onClick: () => void api.openSavedFile(r.file.path) });
      onClose();
    },
  });
  return (
    <Dialog
      open
      onClose={onClose}
      width={500}
      title="Back up now"
      description="Choose a passphrase, then where to save the backup file."
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={save.isPending || !pass || pass !== again} onClick={() => save.mutate(pass)}>
            {save.isPending && <Loader2 size={15} className="animate-spin" />} Choose location & save
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <Field label="Passphrase" error={save.fieldErrors.passphrase} hint="At least 8 characters. A few unrelated words are easy to remember and hard to guess.">
          {(p) => <TextInput id={p.id} invalid={p.invalid} type="password" autoFocus autoComplete="new-password" value={pass} onChange={(e) => setPass(e.target.value)} />}
        </Field>
        <Field label="Type it again" error={mismatch ? "The passphrases don't match." : undefined}>
          {(p) => <TextInput id={p.id} invalid={p.invalid} type="password" autoComplete="new-password" value={again} onChange={(e) => setAgain(e.target.value)} />}
        </Field>
        <p className="flex gap-2 rounded-xl bg-warning-soft px-3 py-2.5 text-[12.5px] text-fg">
          <AlertTriangle size={15} className="mt-px shrink-0 text-warning" aria-hidden />
          Without this passphrase the backup can't be opened, by you or anyone else. Store it somewhere safe, away from the backup.
        </p>
      </div>
    </Dialog>
  );
}

function SetLockDialog({ change, idle, onClose }: { change: boolean; idle: number; onClose: () => void }) {
  const { setLock } = useSession();
  const [current, setCurrent] = useState("");
  const [pin, setPin] = useState("");
  const [again, setAgain] = useState("");
  const [minutes, setMinutes] = useState(idle);
  const [code, setCode] = useState<string | null>(null);
  const mismatch = again.length > 0 && pin !== again;
  const save = useSave((x: { pin: string; minutes: number; current: string }) => api.setAppLock(x.pin, x.minutes, x.current), {
    invalidate: [],
    success: change ? "PIN changed" : "App lock turned on",
    onSuccess: (r) => {
      setCode(r.recoveryCode);
      setLock(r.status);
    },
  });

  if (code) {
    return (
      <Dialog
        open
        onClose={onClose}
        width={460}
        title="Save your recovery code"
        description="If you forget the PIN, this code unlocks AccountsVU and turns the lock off. It's shown only now."
        footer={<Button variant="primary" onClick={onClose}>I've saved it</Button>}
      >
        <div className="num select-all rounded-xl border border-line bg-surface-2 py-4 text-center text-[22px] font-semibold tracking-[0.12em]">{code}</div>
        <p className="mt-3 text-[12.5px] text-muted">Write it down or keep it in a password manager, not on this computer.</p>
      </Dialog>
    );
  }

  return (
    <Dialog
      open
      onClose={onClose}
      width={460}
      title={change ? "Change PIN" : "Turn on app lock"}
      description="A PIN of 4–12 digits, or a password of at least 6 characters."
      footer={
        <>
          <Button onClick={onClose}>Cancel</Button>
          <Button variant="primary" disabled={save.isPending || !pin || pin !== again || (change && !current)} onClick={() => save.mutate({ pin, minutes, current })}>
            {save.isPending && <Loader2 size={15} className="animate-spin" />} {change ? "Change PIN" : "Turn on"}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        {change && (
          <Field label="Current PIN" error={save.fieldErrors.currentPin}>
            {(p) => <TextInput id={p.id} invalid={p.invalid} type="password" autoFocus autoComplete="current-password" value={current} onChange={(e) => setCurrent(e.target.value)} />}
          </Field>
        )}
        <Field label={change ? "New PIN or password" : "PIN or password"} error={save.fieldErrors.pin}>
          {(p) => <TextInput id={p.id} invalid={p.invalid} type="password" autoFocus={!change} autoComplete="new-password" value={pin} onChange={(e) => setPin(e.target.value)} />}
        </Field>
        <Field label="Type it again" error={mismatch ? "They don't match." : undefined}>
          {(p) => <TextInput id={p.id} invalid={p.invalid} type="password" autoComplete="new-password" value={again} onChange={(e) => setAgain(e.target.value)} />}
        </Field>
        <Field label="Lock automatically" error={save.fieldErrors.idleMinutes}>
          {(p) => (
            <Select id={p.id} value={minutes} onChange={(e) => setMinutes(Number(e.target.value))}>
              {IDLE_CHOICES.map((m) => (
                <option key={m} value={m}>{idleLabel(m)}</option>
              ))}
            </Select>
          )}
        </Field>
      </div>
    </Dialog>
  );
}

function RemoveLockDialog({ onClose }: { onClose: () => void }) {
  const { setLock } = useSession();
  const [pin, setPin] = useState("");
  const save = useSave((p: string) => api.removeAppLock(p), {
    invalidate: [],
    success: "App lock turned off",
    onSuccess: (s) => {
      setLock(s);
      onClose();
    },
  });
  return (
    <Dialog
      open
      onClose={onClose}
      width={420}
      title="Turn off app lock?"
      description="AccountsVU will open without a PIN."
      footer={
        <>
          <Button onClick={onClose}>Keep it on</Button>
          <Button variant="primary" disabled={save.isPending || !pin} onClick={() => save.mutate(pin)}>
            Turn off
          </Button>
        </>
      }
    >
      <Field label="Current PIN" error={save.fieldErrors.pin}>
        {(p) => <TextInput id={p.id} invalid={p.invalid} type="password" autoFocus value={pin} onChange={(e) => setPin(e.target.value)} />}
      </Field>
    </Dialog>
  );
}
