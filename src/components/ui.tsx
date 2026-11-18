import clsx from "clsx";
import type { ButtonHTMLAttributes, CSSProperties, ReactNode } from "react";

export function PageHeader({ title, subtitle, actions }: { title: string; subtitle?: ReactNode; actions?: ReactNode }) {
  return (
    <div className="mb-6 flex items-end justify-between gap-4">
      <div>
        <h1 className="text-display">{title}</h1>
        {subtitle && <p className="mt-1 text-[14px] text-muted">{subtitle}</p>}
      </div>
      {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
    </div>
  );
}

export function Card({ className, style, children }: { className?: string; style?: CSSProperties; children: ReactNode }) {
  return (
    <section style={style} className={clsx("rounded-2xl border border-line bg-surface shadow-card", className)}>
      {children}
    </section>
  );
}

type ButtonVariant = "primary" | "secondary" | "ghost";

export function Button({
  variant = "secondary",
  className,
  children,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: ButtonVariant }) {
  return (
    <button
      type="button"
      {...rest}
      className={clsx(
        "pressable inline-flex h-9 items-center justify-center gap-2 rounded-[10px] px-3.5 text-[13.5px] font-medium disabled:cursor-not-allowed disabled:opacity-50",
        variant === "primary" &&
          "bg-accent-fill text-white shadow-[inset_0_1px_0_rgba(255,255,255,0.15),0_1px_2px_rgba(0,0,0,0.12)] hover:bg-[var(--accent-fill-hover)]",
        variant === "secondary" && "border border-line bg-surface text-fg shadow-card hover:bg-surface-2",
        variant === "ghost" && "text-accent hover:bg-accent-soft",
        className,
      )}
    >
      {children}
    </button>
  );
}

type Tone = "neutral" | "accent" | "success" | "warning" | "danger";

/** Status pill — always text (+ optional icon), never colour alone (spec §6.4). */
export function Badge({ tone = "neutral", icon, children }: { tone?: Tone; icon?: ReactNode; children: ReactNode }) {
  return (
    <span
      className={clsx(
        "inline-flex h-[22px] items-center gap-1 rounded-full px-2 text-[11.5px] font-semibold",
        tone === "neutral" && "bg-active text-muted",
        tone === "accent" && "bg-accent-soft text-accent",
        tone === "success" && "bg-success-soft text-success",
        tone === "warning" && "bg-warning-soft text-warning",
        tone === "danger" && "bg-danger-soft text-danger",
      )}
    >
      {icon}
      {children}
    </span>
  );
}

/** Segmented control for switching a list filter. Instant: no animation on a frequent action. */
export function Segmented<T extends string>({
  value,
  onChange,
  options,
  label,
}: {
  value: T;
  onChange: (v: T) => void;
  options: { value: T; label: string; count?: number }[];
  label: string;
}) {
  return (
    <div role="tablist" aria-label={label} className="inline-flex rounded-[10px] bg-active p-0.5">
      {options.map((o) => (
        <button
          key={o.value}
          type="button"
          role="tab"
          aria-selected={o.value === value}
          onClick={() => onChange(o.value)}
          className={clsx(
            "h-8 rounded-[8px] px-3 text-[13px] font-medium",
            o.value === value ? "bg-surface text-fg shadow-card" : "text-muted hover:text-fg",
          )}
        >
          {o.label}
          {o.count != null && <span className="num ml-1.5 text-[12px] text-subtle">{o.count}</span>}
        </button>
      ))}
    </div>
  );
}

/** Empty state: one clear sentence and the next action. */
export function EmptyState({ icon, title, body, actions }: { icon: ReactNode; title: string; body: string; actions?: ReactNode }) {
  return (
    <Card className="flex flex-col items-center px-6 py-14 text-center">
      <span className="mb-4 grid size-12 place-items-center rounded-2xl bg-accent-soft text-accent">{icon}</span>
      <h2 className="text-title">{title}</h2>
      <p className="mt-1.5 max-w-[420px] text-[13.5px] text-muted">{body}</p>
      {actions && <div className="mt-5 flex gap-2">{actions}</div>}
    </Card>
  );
}
