import type { ReactNode } from "react";
import { useSearchParams } from "react-router";
import clsx from "clsx";
import { PageHeader } from "../../components/ui";
import { BusinessTab } from "./BusinessTab";
import { GstTab } from "./GstTab";
import { NumberingTab } from "./NumberingTab";
import { AccountsTab } from "./AccountsTab";
import { ActivityTab } from "./ActivityTab";
import { GeneralTab } from "./GeneralTab";
import { PeriodsTab } from "./PeriodsTab";
import { SecurityTab } from "./SecurityTab";
import { HsnDirectoryTab } from "./HsnDirectoryTab";

type Section = { id: string; label: string; el: ReactNode };
type Group = { id: string; label: string; sections: Section[] };

/**
 * Settings are grouped into a few tabs; a tab with more than one section shows
 * a switch below the tab bar. The URL keeps the section id (`?tab=hsn`), so
 * links into any section keep working.
 */
const GROUPS: Group[] = [
  { id: "general", label: "General", sections: [{ id: "general", label: "Appearance & about", el: <GeneralTab /> }] },
  {
    id: "business",
    label: "Business",
    sections: [
      { id: "business", label: "Profile", el: <BusinessTab /> },
      { id: "numbering", label: "Numbering", el: <NumberingTab /> },
    ],
  },
  {
    id: "gst",
    label: "GST",
    sections: [
      { id: "gst", label: "Registrations", el: <GstTab /> },
      { id: "hsn", label: "HSN/SAC directory", el: <HsnDirectoryTab /> },
    ],
  },
  {
    id: "accounts",
    label: "Accounts",
    sections: [
      { id: "accounts", label: "Chart of accounts", el: <AccountsTab /> },
      { id: "periods", label: "Books & periods", el: <PeriodsTab /> },
    ],
  },
  {
    id: "data",
    label: "Data & security",
    sections: [
      { id: "security", label: "Backup & security", el: <SecurityTab /> },
      { id: "activity", label: "Activity log", el: <ActivityTab /> },
    ],
  },
];

function find(tab: string | null): { group: Group; section: Section } {
  for (const group of GROUPS) {
    const section = group.id === tab ? group.sections[0] : group.sections.find((s) => s.id === tab);
    if (section) return { group, section };
  }
  return { group: GROUPS[0], section: GROUPS[0].sections[0] };
}

/** Settings (spec §6.3). Tabs are high-frequency navigation, so they switch instantly. */
export function Settings() {
  const [params, setParams] = useSearchParams();
  const { group, section } = find(params.get("tab"));
  const open = (id: string) => setParams({ tab: id }, { replace: true });

  return (
    <>
      <PageHeader title="Settings" />
      <div role="tablist" aria-label="Settings" className="mb-5 flex gap-1 overflow-x-auto border-b border-line">
        {GROUPS.map((g) => {
          const selected = g.id === group.id;
          return (
            <button
              key={g.id}
              role="tab"
              type="button"
              aria-selected={selected}
              onClick={() => open(g.sections[0].id)}
              className={clsx(
                "-mb-px shrink-0 whitespace-nowrap border-b-2 px-3 pb-2.5 pt-1 text-[13.5px] font-medium",
                selected ? "border-accent-fill text-fg" : "border-transparent text-muted hover:text-fg",
              )}
            >
              {g.label}
            </button>
          );
        })}
      </div>
      {group.sections.length > 1 ? (
        <div className="lg:grid lg:grid-cols-[188px_1fr] lg:gap-8">
          <nav
            role="tablist"
            aria-label={`${group.label} sections`}
            aria-orientation="vertical"
            className="mb-6 flex gap-1 overflow-x-auto lg:sticky lg:top-4 lg:mb-0 lg:flex-col lg:self-start lg:overflow-visible"
          >
            {group.sections.map((s) => {
              const selected = s.id === section.id;
              return (
                <button
                  key={s.id}
                  role="tab"
                  type="button"
                  aria-selected={selected}
                  onClick={() => open(s.id)}
                  className={clsx(
                    "h-9 shrink-0 whitespace-nowrap rounded-[9px] px-3 text-left text-[13.5px] font-medium",
                    selected ? "bg-active text-fg" : "text-muted hover:bg-surface-2 hover:text-fg",
                  )}
                >
                  {s.label}
                </button>
              );
            })}
          </nav>
          <div role="tabpanel" className="@container min-w-0">
            {section.el}
          </div>
        </div>
      ) : (
        <div role="tabpanel" className="@container">
          {section.el}
        </div>
      )}
    </>
  );
}
