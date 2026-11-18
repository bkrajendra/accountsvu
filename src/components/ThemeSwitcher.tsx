import { Monitor, Moon, Sun } from "lucide-react";
import clsx from "clsx";
import { useTheme, type ThemePreference } from "../lib/theme";

const OPTIONS: { value: ThemePreference; label: string; icon: typeof Sun }[] = [
  { value: "light", label: "Light", icon: Sun },
  { value: "dark", label: "Dark", icon: Moon },
  { value: "system", label: "System", icon: Monitor },
];

/**
 * Segmented Light / Dark / System control. The thumb slides between
 * segments (state indication, 220ms strong ease-out, transform only);
 * reduced-motion users get an instant move.
 */
export function ThemeSwitcher({ showLabels = false }: { showLabels?: boolean }) {
  const { preference, setPreference } = useTheme();
  const index = OPTIONS.findIndex((o) => o.value === preference);

  return (
    <div
      role="radiogroup"
      aria-label="Appearance"
      className="relative grid grid-cols-3 rounded-[10px] bg-active p-0.5"
    >
      <span
        aria-hidden
        data-motion="slide"
        className="absolute inset-y-0.5 left-0.5 rounded-lg bg-surface shadow-card transition-transform duration-[220ms] ease-out-strong motion-reduce:transition-none dark:bg-surface-2"
        style={{ width: "calc((100% - 4px) / 3)", transform: `translateX(${index * 100}%)` }}
      />
      {OPTIONS.map(({ value, label, icon: Icon }) => {
        const selected = value === preference;
        return (
          <button
            key={value}
            type="button"
            role="radio"
            aria-checked={selected}
            aria-label={label}
            title={label}
            onClick={() => setPreference(value)}
            className={clsx(
              "pressable relative z-10 flex items-center justify-center gap-1.5 rounded-lg font-medium",
              showLabels ? "h-8 px-3 text-[13px]" : "h-7 w-8",
              selected ? "text-fg" : "text-muted hover:text-fg",
            )}
          >
            <Icon size={15} strokeWidth={selected ? 2.2 : 1.8} />
            {showLabels && <span>{label}</span>}
          </button>
        );
      })}
    </div>
  );
}
