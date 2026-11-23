import type { ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import clsx from "clsx";
import { Check, Coffee, Download, ExternalLink, Loader2, RefreshCw } from "lucide-react";
import { api, keys, runningInTauri } from "../../lib/api";
import { useTheme, type ThemePreference } from "../../lib/theme";
import { Badge, Button, Card } from "../../components/ui";
import { useUpdater } from "../../lib/updater";
import { SettingsSection as Section } from "./SettingsSection";

export function GeneralTab() {
  const { data: health } = useQuery({ queryKey: keys.health, queryFn: api.health });
  const { data: rules } = useQuery({ queryKey: ["rulePackInfo"], queryFn: api.rulePackInfo, staleTime: Infinity });

  return (
    <>
      <Section title="Appearance" description="Choose how AccountsVU looks. System follows your Windows setting.">
        <ThemeCards />
      </Section>

      <Section title="About" description="Version and storage details for support.">
        <Card>
          <dl className="divide-y divide-line text-[13.5px]">
            <Row label="Installed version" value={health?.appVersion ?? "—"} />
            <UpdateRow />
            <Row
              label="GST rules"
              value={rules ? <span title={rules.sources.map((x) => x.title).join(" · ")}>{`${rules.version} · ${rules.hsnEntries} HSN/SAC entries`}</span> : "—"}
            />
            <Row label="Database schema" value={health ? `v${health.schemaVersion}` : "—"} />
            <Row label="Data file" value={<span className="selectable break-all">{health?.database ?? "—"}</span>} />
            <Row
              label="Runtime"
              value={runningInTauri ? <Badge tone="success">Desktop</Badge> : <Badge tone="warning">Browser preview</Badge>}
            />
          </dl>
        </Card>
        <Card className="mt-4 flex items-center gap-4 px-5 py-4">
          <span className="grid size-9 shrink-0 place-items-center rounded-[10px] bg-[#FFDD00] text-[#1d1d1f]">
            <Coffee size={18} strokeWidth={2} />
          </span>
          <div className="min-w-0 flex-1 text-[13.5px]">
            <div className="font-medium">AccountsVU is free and stays free</div>
            <div className="text-muted">If it saves you time every month, you can say thanks with a coffee. It helps keep the work going.</div>
          </div>
          <Button className="shrink-0" onClick={() => void api.openSupportPage()}>
            Buy me a coffee <ExternalLink size={13} />
          </Button>
        </Card>
      </Section>
    </>
  );
}

function UpdateRow() {
  const { phase, info, checkedAt, error, progress, check, openDialog } = useUpdater();
  if (!runningInTauri && phase === "idle") return null;
  const busy = phase === "downloading" || phase === "installing";
  const checked = checkedAt ? checkedAt.toLocaleTimeString([], { hour: "numeric", minute: "2-digit" }) : null;

  let value: ReactNode;
  if (phase === "checking" || phase === "idle") {
    value = (
      <span className="inline-flex items-center gap-2 text-muted">
        <Loader2 size={14} className="animate-spin" /> Checking…
      </span>
    );
  } else if (info && (phase === "available" || busy || phase === "error")) {
    const pct = progress?.total ? Math.round((progress.downloaded / progress.total) * 100) : null;
    value = (
      <span className="inline-flex items-center gap-3">
        <Badge tone="accent">{info.version} available</Badge>
        <Button variant="primary" className="h-8" onClick={openDialog}>
          {busy ? <Loader2 size={14} className="animate-spin" /> : <Download size={14} />}
          {busy ? (pct !== null ? `Downloading ${pct}%` : "Updating…") : "Update"}
        </Button>
      </span>
    );
  } else if (phase === "error") {
    value = (
      <span className="inline-flex items-center gap-3">
        <span className="font-normal text-danger">{error ?? "Couldn't check for updates."}</span>
        <Button className="h-8" onClick={() => void check()}>
          <RefreshCw size={14} /> Try again
        </Button>
      </span>
    );
  } else {
    value = (
      <span className="inline-flex items-center gap-3">
        <Badge tone="success" icon={<Check size={12} strokeWidth={3} />}>
          Up to date
        </Badge>
        {checked && <span className="font-normal text-muted">checked {checked}</span>}
        <Button className="h-8" onClick={() => void check()}>
          <RefreshCw size={14} /> Check again
        </Button>
      </span>
    );
  }
  return <Row label="Latest version" value={value} />;
}

function Row({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-6 px-5 py-3">
      <dt className="text-muted">{label}</dt>
      <dd className="text-right font-medium">{value}</dd>
    </div>
  );
}

const THEMES: { value: ThemePreference; label: string }[] = [
  { value: "light", label: "Light" },
  { value: "dark", label: "Dark" },
  { value: "system", label: "System" },
];

function ThemeCards() {
  const { preference, setPreference } = useTheme();
  return (
    <div role="radiogroup" aria-label="Theme" className="grid grid-cols-3 gap-4">
      {THEMES.map(({ value, label }) => {
        const selected = preference === value;
        return (
          <button
            key={value}
            type="button"
            role="radio"
            aria-checked={selected}
            onClick={() => setPreference(value)}
            className="pressable group flex flex-col items-start gap-2.5 text-left"
          >
            <span
              className={clsx(
                "block w-full overflow-hidden rounded-xl border-2 transition-[border-color,box-shadow] duration-150",
                selected ? "border-accent-fill shadow-[var(--focus-ring)]" : "border-line group-hover:border-line-strong",
              )}
            >
              <ThemePreview variant={value} />
            </span>
            <span className="flex items-center gap-1.5 text-[13px] font-medium">
              <span
                className={clsx(
                  "grid size-4 place-items-center rounded-full border",
                  selected ? "border-accent-fill bg-accent-fill text-white" : "border-line-strong",
                )}
              >
                {selected && <Check size={10} strokeWidth={3.5} />}
              </span>
              {label}
            </span>
          </button>
        );
      })}
    </div>
  );
}

/** Miniature window preview; System shows light and dark side by side. */
function ThemePreview({ variant }: { variant: ThemePreference }) {
  const mini = (dark: boolean) => (
    <div className={clsx("flex h-full", dark ? "bg-[#161618]" : "bg-[#f5f5f7]")}>
      <div className={clsx("w-[28%] space-y-1.5 p-2", dark ? "bg-[#1e1e21]" : "bg-[#ececef]")}>
        <div className="h-1.5 w-3/4 rounded-full bg-brand" />
        <div className={clsx("h-1.5 w-2/3 rounded-full", dark ? "bg-white/20" : "bg-black/15")} />
        <div className={clsx("h-1.5 w-1/2 rounded-full", dark ? "bg-white/20" : "bg-black/15")} />
      </div>
      <div className="flex-1 space-y-1.5 p-2">
        <div className={clsx("h-6 rounded-md", dark ? "bg-[#1f1f22]" : "bg-white")} />
        <div className="flex gap-1.5">
          <div className={clsx("h-5 flex-1 rounded-md", dark ? "bg-[#1f1f22]" : "bg-white")} />
          <div className={clsx("h-5 flex-1 rounded-md", dark ? "bg-[#1f1f22]" : "bg-white")} />
        </div>
      </div>
    </div>
  );
  return (
    <div className="h-[86px]">
      {variant === "system" ? (
        <div className="grid h-full grid-cols-2">
          {mini(false)}
          {mini(true)}
        </div>
      ) : (
        mini(variant === "dark")
      )}
    </div>
  );
}
