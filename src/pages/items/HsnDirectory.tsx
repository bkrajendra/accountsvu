import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { useQuery } from "@tanstack/react-query";
import clsx from "clsx";
import { BookOpen, Loader2, Search } from "lucide-react";
import { Link } from "react-router";
import { api, type CodeRef, type DirectoryHit, type DirectoryKind, type TaxCategory } from "../../lib/api";
import { formatRate } from "../../lib/format";
import { Badge, Button, Segmented } from "../../components/ui";
import { Dialog } from "../../components/Dialog";
import { useDebounced } from "../../components/editor";

const CATEGORY_WORDS: Record<TaxCategory, string> = {
  taxable: "taxable",
  exempt: "exempt",
  nil_rated: "nil-rated",
  non_gst: "outside GST",
};

type KindFilter = "all" | DirectoryKind;

/** Search the HSN/SAC directory (or the rule pack before the first sync). */
export function useHsnSearch(query: string, kind: KindFilter, limit = 50) {
  const q = useDebounced(query.trim(), 200);
  const result = useQuery({
    queryKey: ["hsnDirectorySearch", q, kind, limit],
    queryFn: () => api.searchHsnDirectory({ query: q, kind: kind === "all" ? null : kind, limit }),
    enabled: q.length > 0,
    staleTime: 60_000,
    placeholderData: (prev) => prev,
  });
  return { ...result, query: q };
}

function Ancestor({ label, r }: { label: string; r: CodeRef | null }) {
  if (!r) return null;
  return (
    <div className="flex min-w-0 gap-2">
      <dt className="w-[92px] shrink-0 text-subtle">{label}</dt>
      <dd className="min-w-0 truncate" title={`${r.code} ${r.description}`}>
        <span className="num text-muted">{r.code}</span> {r.description}
      </dd>
    </div>
  );
}

/** One directory entry: the code, its description and where it sits (chapter, heading, sub-heading). */
export function HitRow({ hit, active, onSelect }: { hit: DirectoryHit; active?: boolean; onSelect?: () => void }) {
  const rate = hit.rate;
  const body = (
    <>
      <div className="flex items-start gap-3">
        <span className="num w-[78px] shrink-0 pt-px text-[14px] font-semibold tracking-wide">{hit.code}</span>
        <div className="min-w-0 flex-1">
          <p className="line-clamp-3 text-[13.5px]" title={hit.description}>
            {hit.description}
          </p>
          <dl className="mt-1 space-y-0.5 text-[12px]">
            <Ancestor label="Chapter" r={hit.chapter} />
            <Ancestor label="Heading" r={hit.heading} />
            <Ancestor label="Sub-heading" r={hit.subheading} />
          </dl>
        </div>
        <div className="flex shrink-0 flex-col items-end gap-1">
          <Badge tone={hit.kind === "service" ? "accent" : "neutral"}>{hit.kind === "service" ? "SAC" : "HSN"}</Badge>
          {hit.delisted && <Badge tone="warning">Not in latest list</Badge>}
          {rate && (
            <span
              className="text-[12px] text-muted"
              title={`GST rules entry ${rate.ruleCode}${rate.conditional ? ": the rate depends on value, packaging or buyer" : ""}`}
            >
              {rate.taxCategory === "taxable" ? formatRate(rate.rateBp) : CATEGORY_WORDS[rate.taxCategory]}
              {rate.conditional ? "*" : ""}
            </span>
          )}
        </div>
      </div>
    </>
  );
  if (!onSelect) return <li className="px-4 py-3">{body}</li>;
  return (
    <li role="option" aria-selected={active} data-active={active || undefined}>
      <button
        type="button"
        tabIndex={-1}
        onClick={onSelect}
        className={clsx("block w-full px-4 py-3 text-left", active ? "bg-accent-soft" : "hover:bg-surface-2")}
      >
        {body}
      </button>
    </li>
  );
}

/** The search box, HSN/SAC filter and results, shared by Settings and the item picker. */
export function HsnSearchPanel({
  initialQuery = "",
  initialKind = "all",
  onSelect,
  autoFocus,
  listClassName,
}: {
  initialQuery?: string;
  initialKind?: KindFilter;
  onSelect?: (hit: DirectoryHit) => void;
  autoFocus?: boolean;
  listClassName?: string;
}) {
  const [text, setText] = useState(initialQuery);
  const [kind, setKind] = useState<KindFilter>(initialKind);
  const [active, setActive] = useState(0);
  const { data, isFetching, query } = useHsnSearch(text, kind);
  const listRef = useRef<HTMLUListElement>(null);
  const hits = query ? (data?.hits ?? []) : [];

  useEffect(() => setActive(0), [query, kind]);
  useEffect(() => {
    listRef.current?.querySelector("[data-active]")?.scrollIntoView({ block: "nearest" });
  }, [active]);

  function onKey(e: KeyboardEvent<HTMLInputElement>) {
    if (!onSelect || !hits.length) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((a) => Math.min(a + 1, hits.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => Math.max(a - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      const h = hits[active];
      if (h) onSelect(h);
    }
  }

  return (
    <div className="flex min-h-0 flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2">
        <div className="relative min-w-[220px] flex-1">
          <Search size={15} className="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-subtle" aria-hidden />
          <input
            type="search"
            autoFocus={autoFocus}
            data-autofocus={autoFocus || undefined}
            value={text}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={onKey}
            placeholder="Code or words, e.g. 0202 or frozen meat"
            aria-label="Search HSN/SAC codes"
            role={onSelect ? "combobox" : undefined}
            aria-expanded={onSelect ? hits.length > 0 : undefined}
            aria-controls={onSelect ? "hsn-results" : undefined}
            className="h-9 w-full rounded-[9px] border border-line-strong bg-surface pl-9 pr-8 text-[13.5px] placeholder:text-subtle focus:border-accent-fill focus:shadow-[var(--focus-ring)] focus:outline-none [&::-webkit-search-cancel-button]:hidden"
          />
          {isFetching && <Loader2 size={14} className="absolute right-2.5 top-1/2 -translate-y-1/2 animate-spin text-subtle" aria-hidden />}
        </div>
        <Segmented<KindFilter>
          label="Code type"
          value={kind}
          onChange={setKind}
          options={[
            { value: "all", label: "All" },
            { value: "hsn", label: "HSN · goods" },
            { value: "sac", label: "SAC · services" },
          ]}
        />
      </div>

      {data?.source === "rule_pack" && query && (
        <p className="flex items-start gap-2 rounded-lg bg-surface-2 px-3 py-2 text-[12.5px] text-muted">
          <BookOpen size={14} className="mt-[2px] shrink-0" aria-hidden />
          <span>
            Showing the GST rules' headings only.{" "}
            <Link to="/settings?tab=hsn" className="font-medium text-accent hover:underline">
              Sync the HSN/SAC directory
            </Link>{" "}
            to search every code.
          </span>
        </p>
      )}

      {!query ? (
        <p className="px-1 py-6 text-center text-[13px] text-muted">Type a code (any number of digits) or a few words from its description.</p>
      ) : hits.length === 0 ? (
        <p className="px-1 py-6 text-center text-[13px] text-muted">{isFetching ? "Searching…" : `No codes match “${query}”.`}</p>
      ) : (
        <>
          <ul
            ref={listRef}
            id="hsn-results"
            role={onSelect ? "listbox" : undefined}
            aria-label="HSN/SAC codes"
            className={clsx("divide-y divide-line overflow-y-auto rounded-xl border border-line", listClassName)}
          >
            {hits.map((h, i) => (
              <HitRow key={h.code} hit={h} active={i === active} onSelect={onSelect ? () => onSelect(h) : undefined} />
            ))}
          </ul>
          <p className="text-[12px] text-subtle">
            {data?.more ? `First ${hits.length} matches; add words or digits to narrow it down.` : `${hits.length} ${hits.length === 1 ? "match" : "matches"}.`}{" "}
            Rates shown are from AccountsVU's GST rules, not the directory{hits.some((h) => h.rate?.conditional) ? "; * depends on value, packaging or buyer" : ""}.
          </p>
        </>
      )}
    </div>
  );
}

/** Item picker: find a code and put it in the item's HSN/SAC field. */
export function HsnFinder({
  initialQuery,
  kind,
  onPick,
  onClose,
}: {
  initialQuery: string;
  kind: DirectoryKind;
  onPick: (hit: DirectoryHit) => void;
  onClose: () => void;
}) {
  return (
    <Dialog
      open
      onClose={onClose}
      width={720}
      title="Find HSN/SAC code"
      description="Search the HSN/SAC directory. A lookup aid: check the code against your supplier's invoice or the tariff when in doubt."
      footer={<Button onClick={onClose}>Cancel</Button>}
    >
      <HsnSearchPanel initialQuery={initialQuery} initialKind={kind} onSelect={onPick} autoFocus listClassName="max-h-[min(52vh,460px)]" />
    </Dialog>
  );
}
