import { forwardRef } from "react";
import { Building2, ChevronDown, Lock, Search } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router";
import { ThemeSwitcher } from "./ThemeSwitcher";
import { api, keys } from "../lib/api";
import { useSession } from "../lib/session";


/**
 * Top command/search area plus the GSTIN + financial-year context selector
 * (spec §6.1: the current GSTIN and FY are always visible).
 */
export const TopBar = forwardRef<HTMLInputElement>(function TopBar(_props, searchRef) {
  const navigate = useNavigate();
  const { data: profile } = useQuery({ queryKey: keys.profile, queryFn: api.businessProfile });
  const gstin = profile?.registrations.find((r) => r.status === "active")?.gstin;
  const fyLabel = profile?.currentFinancialYear.label.replace("-", "–") ?? "";
  const { lock, lockNow } = useSession();

  return (
    <header className="material sticky top-0 z-20 flex h-14 shrink-0 items-center gap-3 border-b border-line px-5">
      <label className="group relative flex h-9 w-full max-w-[420px] items-center">
        <Search size={15} className="pointer-events-none absolute left-3 text-subtle" aria-hidden />
        <input
          ref={searchRef}
          type="search"
          placeholder="Search invoices, parties, items…"
          aria-label="Search"
          className="h-full w-full rounded-[10px] border border-transparent bg-active pl-9 pr-14 text-[13.5px] text-fg placeholder:text-muted focus:border-line-strong focus:bg-surface focus:shadow-[var(--focus-ring)] focus:outline-none"
        />
        <kbd className="pointer-events-none absolute right-2.5 rounded-[5px] border border-line bg-surface px-1.5 py-px font-sans text-[11px] font-medium text-muted group-focus-within:hidden">
          Ctrl K
        </kbd>
      </label>

      <div className="ml-auto flex items-center gap-3">
        <button
          type="button"
          className="pressable flex h-9 items-center gap-2 rounded-[10px] border border-line bg-surface px-3 text-[13px] shadow-card hover:bg-surface-2"
          title={profile?.organisation ? "GST registrations" : "Set up your business"}
          onClick={() => navigate(profile?.organisation ? "/settings?tab=gst" : "/setup")}
        >
          <Building2 size={15} className="text-muted" aria-hidden />
          <span className={gstin ? "num font-medium" : "font-medium text-muted"}>{gstin ?? "No GSTIN yet"}</span>
          <span className="h-4 w-px bg-line-strong" aria-hidden />
          <span className="font-medium">{fyLabel}</span>
          <ChevronDown size={14} className="text-subtle" aria-hidden />
        </button>
        {lock.enabled && (
          <button
            type="button"
            title="Lock AccountsVU"
            aria-label="Lock AccountsVU"
            onClick={() => void lockNow()}
            className="pressable grid size-9 place-items-center rounded-[10px] border border-line bg-surface text-muted shadow-card hover:bg-surface-2 hover:text-fg"
          >
            <Lock size={15} />
          </button>
        )}
        <ThemeSwitcher />
      </div>
    </header>
  );
});
