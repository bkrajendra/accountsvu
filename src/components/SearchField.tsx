import { Search, X } from "lucide-react";

/** List filter input with a clear button. */
export function SearchField({ value, onChange, placeholder }: { value: string; onChange: (v: string) => void; placeholder: string }) {
  return (
    <div className="relative w-[280px]">
      <Search size={15} className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-subtle" aria-hidden />
      <input
        type="search"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        aria-label={placeholder}
        className="h-9 w-full rounded-[9px] border border-line-strong bg-surface pl-9 pr-8 text-[13.5px] placeholder:text-subtle focus:border-accent-fill focus:shadow-[var(--focus-ring)] focus:outline-none [&::-webkit-search-cancel-button]:hidden"
      />
      {value && (
        <button
          type="button"
          onClick={() => onChange("")}
          aria-label="Clear search"
          className="absolute right-1.5 top-1/2 grid size-6 -translate-y-1/2 place-items-center rounded-md text-subtle hover:bg-hover hover:text-fg"
        >
          <X size={14} />
        </button>
      )}
    </div>
  );
}
