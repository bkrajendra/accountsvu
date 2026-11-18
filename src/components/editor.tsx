import { useEffect, useState } from "react";
import clsx from "clsx";
import { TextInput } from "./form";

/** Debounce a value so a live calculation doesn't run on every keystroke. */
export function useDebounced<T>(value: T, ms: number): T {
  const [v, setV] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setV(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return v;
}

/** Compact input for a document line grid. */
export function CellInput({ value, onChange, invalid, numeric, label }: { value: string; onChange: (v: string) => void; invalid?: boolean; numeric?: boolean; label: string }) {
  return (
    <TextInput
      aria-label={label}
      invalid={invalid}
      value={value}
      inputMode={numeric ? "decimal" : undefined}
      onFocus={(e) => e.target.select()}
      onChange={(e) => onChange(e.target.value)}
      className={clsx("px-2", numeric && "num text-right")}
    />
  );
}
