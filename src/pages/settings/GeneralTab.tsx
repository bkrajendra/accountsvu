import type { ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import clsx from "clsx";
import { Check } from "lucide-react";
import { api, keys, runningInTauri } from "../../lib/api";
import { useTheme, type ThemePreference } from "../../lib/theme";
import { Badge, Card } from "../../components/ui";
import { SettingsSection as Section } from "./SettingsSection";

export function GeneralTab() {
  const { data: health } = useQuery({ queryKey: keys.health, queryFn: api.health });

  return (
    <>
      <Section title="Appearance" description="Choose how AccountsVU looks. System follows your Windows setting.">
        <ThemeCards />
      </Section>

      <Section title="About" description="Version and storage details for support.">
        <Card>
          <dl className="divide-y divide-line text-[13.5px]">
            <Row label="App version" value={health?.appVersion ?? "—"} />
            <Row label="Database schema" value={health ? `v${health.schemaVersion}` : "—"} />
            <Row label="Data file" value={<span className="selectable break-all">{health?.database ?? "—"}</span>} />
            <Row
              label="Runtime"
              value={runningInTauri ? <Badge tone="success">Desktop</Badge> : <Badge tone="warning">Browser preview</Badge>}
            />
          </dl>
        </Card>
      </Section>
    </>
  );
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
        <div className="h-1.5 w-3/4 rounded-full bg-[#0071e3]" />
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
