import { useState, type FormEvent } from "react";
import { Loader2, Lock } from "lucide-react";
import { api, ApiError, type LockStatus } from "../lib/api";
import { BrandMark } from "./Brand";
import { Button } from "./ui";
import { Field, TextInput } from "./form";

/**
 * Shown instead of the app while it is locked. Nothing from the books is
 * rendered or cached behind it.
 */
export function LockScreen({ onUnlocked }: { onUnlocked: (s: LockStatus) => void }) {
  const [mode, setMode] = useState<"pin" | "recovery">("pin");
  const [value, setValue] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!value || busy) return;
    setBusy(true);
    setError(null);
    try {
      onUnlocked(mode === "pin" ? await api.unlockApp(value) : await api.unlockWithRecovery(value));
    } catch (err) {
      setValue("");
      setError(err instanceof ApiError ? (Object.values(err.fields)[0] ?? err.message) : String(err));
    } finally {
      setBusy(false);
    }
  };

  const recovery = mode === "recovery";
  return (
    <main className="grid min-h-screen place-items-center bg-bg px-4">
      <form onSubmit={submit} className="rise-in w-full max-w-[360px] text-center">
        <BrandMark size={44} className="mx-auto" />
        <h1 className="mt-5 flex items-center justify-center gap-2 text-[20px] font-semibold tracking-[-0.01em]">
          <Lock size={17} className="text-muted" aria-hidden /> AccountsVU is locked
        </h1>
        <p className="mt-1.5 text-[13.5px] text-muted">
          {recovery ? "Enter the recovery code you saved when you set the PIN. It turns the lock off." : "Enter your PIN or password to continue."}
        </p>
        <div className="mt-6 text-left">
          <Field label={recovery ? "Recovery code" : "PIN or password"} error={error ?? undefined}>
            {(p) => (
              <TextInput
                id={p.id}
                invalid={p.invalid}
                type={recovery ? "text" : "password"}
                autoFocus
                autoComplete="off"
                spellCheck={false}
                placeholder={recovery ? "XXXX-XXXX-XXXX-XXXX" : undefined}
                value={value}
                onChange={(e) => setValue(e.target.value)}
                className={recovery ? "num uppercase tracking-[0.08em]" : "text-center tracking-[0.3em]"}
              />
            )}
          </Field>
        </div>
        <Button type="submit" variant="primary" className="mt-4 w-full" disabled={!value || busy}>
          {busy && <Loader2 size={15} className="animate-spin" />} {recovery ? "Use recovery code" : "Unlock"}
        </Button>
        <button
          type="button"
          className="mt-4 text-[13px] text-accent hover:underline"
          onClick={() => {
            setMode(recovery ? "pin" : "recovery");
            setValue("");
            setError(null);
          }}
        >
          {recovery ? "Back to PIN" : "Forgot your PIN?"}
        </button>
      </form>
    </main>
  );
}
