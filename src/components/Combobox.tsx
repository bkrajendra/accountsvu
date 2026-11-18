import { useEffect, useId, useMemo, useRef, useState, type KeyboardEvent } from "react";
import clsx from "clsx";
import { ChevronDown } from "lucide-react";

export interface ComboOption {
  id: string;
  label: string;
  /** Second line, e.g. a GSTIN or HSN code. */
  detail?: string;
  /** Extra text matched by search but not shown. */
  keywords?: string;
}

/**
 * Searchable picker (ARIA combobox pattern). Type to filter, ↑/↓ to move,
 * Enter to choose, Esc to close. With `freeText`, whatever is typed is kept
 * as text when no option is chosen (e.g. an ad-hoc invoice line).
 *
 * No open/close animation: this is used many times per invoice.
 */
export function Combobox({
  options,
  value,
  text,
  onSelect,
  onText,
  placeholder,
  invalid,
  id,
  freeText = false,
  className,
  ariaLabel,
  autoFocus,
}: {
  options: ComboOption[];
  /** Selected option id. */
  value: string | null;
  /** Shown when nothing is selected (free-text mode). */
  text?: string;
  onSelect: (id: string | null) => void;
  onText?: (text: string) => void;
  placeholder?: string;
  invalid?: boolean;
  id?: string;
  freeText?: boolean;
  className?: string;
  ariaLabel?: string;
  autoFocus?: boolean;
}) {
  const listId = useId();
  const selected = options.find((o) => o.id === value) ?? null;
  const [query, setQuery] = useState<string | null>(null);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const shown = query ?? selected?.label ?? text ?? "";

  const matches = useMemo(() => {
    const q = (query ?? "").trim().toLowerCase();
    const list = q
      ? options.filter((o) => `${o.label} ${o.detail ?? ""} ${o.keywords ?? ""}`.toLowerCase().includes(q))
      : options;
    return list.slice(0, 50);
  }, [options, query]);

  useEffect(() => setActive(0), [query, open]);

  function choose(o: ComboOption) {
    onSelect(o.id);
    setQuery(null);
    setOpen(false);
  }

  function commitText() {
    if (query === null) return;
    if (freeText) {
      if (selected && query !== selected.label) onSelect(null);
      onText?.(query);
    }
    setQuery(null);
  }

  function onKeyDown(e: KeyboardEvent<HTMLInputElement>) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      if (!open) setOpen(true);
      else setActive((a) => Math.min(a + 1, matches.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => Math.max(a - 1, 0));
    } else if (e.key === "Enter" && open && matches[active] && query !== null) {
      e.preventDefault();
      e.stopPropagation();
      choose(matches[active]);
    } else if (e.key === "Escape" && open) {
      e.preventDefault();
      e.stopPropagation();
      setOpen(false);
      setQuery(null);
    }
  }

  return (
    <div className={clsx("relative", className)}>
      <input
        ref={input}
        id={id}
        role="combobox"
        aria-label={ariaLabel}
        aria-expanded={open}
        aria-controls={listId}
        aria-autocomplete="list"
        aria-invalid={invalid || undefined}
        aria-activedescendant={open && matches[active] ? `${listId}-${active}` : undefined}
        autoComplete="off"
        autoFocus={autoFocus}
        value={shown}
        placeholder={placeholder}
        onChange={(e) => {
          setQuery(e.target.value);
          setOpen(true);
        }}
        onFocus={(e) => e.target.select()}
        onClick={() => setOpen(true)}
        onBlur={() => {
          // Let a click on an option land first.
          setTimeout(() => {
            setOpen(false);
            commitText();
          }, 120);
        }}
        onKeyDown={onKeyDown}
        className={clsx(
          "h-9 w-full rounded-[9px] border bg-surface pl-3 pr-8 text-[13.5px] placeholder:text-subtle focus:shadow-[var(--focus-ring)] focus:outline-none",
          invalid ? "border-danger" : "border-line-strong focus:border-accent-fill",
        )}
      />
      <ChevronDown size={15} className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 text-subtle" aria-hidden />
      {open && (
        <ul
          id={listId}
          role="listbox"
          className="absolute left-0 right-0 top-[calc(100%+4px)] z-30 max-h-[280px] min-w-[260px] overflow-y-auto rounded-xl border border-line bg-surface p-1 shadow-raised"
        >
          {matches.length === 0 ? (
            <li className="px-3 py-2 text-[13px] text-muted">{freeText && query ? `Use “${query}” as the description` : "No matches"}</li>
          ) : (
            matches.map((o, i) => (
              <li
                key={o.id}
                id={`${listId}-${i}`}
                role="option"
                aria-selected={i === active}
                onMouseDown={(e) => {
                  e.preventDefault();
                  choose(o);
                }}
                onMouseEnter={() => setActive(i)}
                className={clsx("cursor-pointer rounded-lg px-3 py-1.5", i === active && "bg-hover")}
              >
                <div className="truncate text-[13.5px]">{o.label}</div>
                {o.detail && <div className="num truncate text-[12px] text-muted">{o.detail}</div>}
              </li>
            ))
          )}
        </ul>
      )}
    </div>
  );
}
