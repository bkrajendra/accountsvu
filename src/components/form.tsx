import clsx from "clsx";
import { useId, type InputHTMLAttributes, type ReactNode, type SelectHTMLAttributes } from "react";
import { AlertCircle, ChevronDown } from "lucide-react";

const control =
  "h-9 w-full rounded-[10px] border bg-surface px-3 text-[13.5px] text-fg placeholder:text-subtle " +
  "transition-[border-color,box-shadow] duration-150 focus:outline-none focus:shadow-[var(--focus-ring)] " +
  "disabled:cursor-not-allowed disabled:bg-surface-2 disabled:text-muted";

interface FieldProps {
  label: string;
  hint?: ReactNode;
  error?: string;
  optional?: boolean;
  className?: string;
  children: (props: { id: string; invalid: boolean; describedBy?: string }) => ReactNode;
}

/** Label + control + inline hint/error (errors shown inline, spec §6.3). */
export function Field({ label, hint, error, optional, className, children }: FieldProps) {
  const id = useId();
  const msgId = `${id}-msg`;
  return (
    <div className={clsx("flex flex-col gap-1.5", className)}>
      <label htmlFor={id} className="flex items-baseline justify-between text-[12.5px] font-medium text-fg">
        {label}
        {optional && <span className="text-[11.5px] font-normal text-muted">Optional</span>}
      </label>
      {children({ id, invalid: !!error, describedBy: error || hint ? msgId : undefined })}
      {error ? (
        <p id={msgId} className="flex items-start gap-1.5 text-[12px] text-danger" role="alert">
          <AlertCircle size={13} className="mt-[2px] shrink-0" aria-hidden />
          {error}
        </p>
      ) : hint ? (
        <p id={msgId} className="text-[12px] text-muted">
          {hint}
        </p>
      ) : null}
    </div>
  );
}

type InputProps = InputHTMLAttributes<HTMLInputElement> & { invalid?: boolean };

export function TextInput({ invalid, className, ...rest }: InputProps) {
  return (
    <input
      {...rest}
      data-autofocus={rest.autoFocus || undefined}
      aria-invalid={invalid || undefined}
      className={clsx(control, invalid ? "border-danger" : "border-line-strong focus:border-accent-fill", className)}
    />
  );
}

type SelectProps = SelectHTMLAttributes<HTMLSelectElement> & { invalid?: boolean };

export function Select({ invalid, className, children, ...rest }: SelectProps) {
  return (
    <div className="relative">
      <select
        {...rest}
        aria-invalid={invalid || undefined}
        className={clsx(
          control,
          "appearance-none pr-8",
          invalid ? "border-danger" : "border-line-strong focus:border-accent-fill",
          className,
        )}
      >
        {children}
      </select>
      <ChevronDown size={15} className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 text-subtle" aria-hidden />
    </div>
  );
}

/** Large tappable choice cards for a radio group. */
export function ChoiceCards<T extends string>({
  value,
  onChange,
  options,
  name,
  columns = 2,
}: {
  value: T;
  onChange: (v: T) => void;
  options: { value: T; title: string; description?: string; icon?: ReactNode }[];
  name: string;
  columns?: 2 | 3;
}) {
  return (
    <div role="radiogroup" aria-label={name} className={clsx("grid gap-3", columns === 3 ? "grid-cols-3" : "grid-cols-2")}>
      {options.map((o) => {
        const selected = o.value === value;
        return (
          <button
            key={o.value}
            type="button"
            role="radio"
            aria-checked={selected}
            aria-label={o.description ? `${o.title} — ${o.description}` : o.title}
            onClick={() => onChange(o.value)}
            className={clsx(
              "pressable flex items-start gap-3 rounded-xl border p-3.5 text-left",
              selected ? "border-accent-fill bg-accent-soft" : "border-line-strong bg-surface hover:bg-surface-2",
            )}
          >
            {o.icon && <span className={clsx("mt-0.5", selected ? "text-accent" : "text-muted")}>{o.icon}</span>}
            <span className="min-w-0 flex-1">
              <span className="block text-[13.5px] font-semibold">{o.title}</span>
              {o.description && <span className="mt-0.5 block text-[12.5px] text-muted">{o.description}</span>}
            </span>
            <span
              aria-hidden
              className={clsx(
                "mt-0.5 grid size-4 shrink-0 place-items-center rounded-full border",
                selected ? "border-accent-fill bg-accent-fill" : "border-line-strong",
              )}
            >
              {selected && <span className="size-1.5 rounded-full bg-white" />}
            </span>
          </button>
        );
      })}
    </div>
  );
}

/** Accessible switch. */
export function Toggle({
  checked,
  onChange,
  label,
  description,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  label: string;
  description?: string;
}) {
  return (
    <label className="flex cursor-pointer items-start justify-between gap-4">
      <span>
        <span className="block text-[13.5px] font-medium">{label}</span>
        {description && <span className="mt-0.5 block text-[12.5px] text-muted">{description}</span>}
      </span>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        onClick={() => onChange(!checked)}
        className={clsx(
          "relative h-[22px] w-[38px] shrink-0 rounded-full transition-colors duration-200",
          checked ? "bg-accent-fill" : "bg-line-strong",
        )}
      >
        <span
          data-motion="slide"
          className="absolute left-[2px] top-[2px] size-[18px] rounded-full bg-white shadow-[0_1px_3px_rgba(0,0,0,0.25)] transition-transform duration-200 ease-out-strong"
          style={{ transform: checked ? "translateX(16px)" : "translateX(0)" }}
        />
      </button>
    </label>
  );
}

/** Two-column responsive form grid. */
export function FormGrid({ children }: { children: ReactNode }) {
  return <div className="grid grid-cols-2 gap-x-4 gap-y-4">{children}</div>;
}
