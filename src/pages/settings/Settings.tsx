import { useSearchParams } from "react-router";
import clsx from "clsx";
import { PageHeader } from "../../components/ui";
import { BusinessTab } from "./BusinessTab";
import { GstTab } from "./GstTab";
import { NumberingTab } from "./NumberingTab";
import { AccountsTab } from "./AccountsTab";
import { ActivityTab } from "./ActivityTab";
import { GeneralTab } from "./GeneralTab";

const TABS = [
  { id: "business", label: "Business", el: <BusinessTab /> },
  { id: "gst", label: "GST registrations", el: <GstTab /> },
  { id: "numbering", label: "Numbering", el: <NumberingTab /> },
  { id: "accounts", label: "Chart of accounts", el: <AccountsTab /> },
  { id: "activity", label: "Activity log", el: <ActivityTab /> },
  { id: "general", label: "Appearance & about", el: <GeneralTab /> },
] as const;

/** Settings (spec §6.3). Tabs are high-frequency navigation, so they switch instantly. */
export function Settings() {
  const [params, setParams] = useSearchParams();
  const active = TABS.find((t) => t.id === params.get("tab")) ?? TABS[0];

  return (
    <>
      <PageHeader title="Settings" />
      <div role="tablist" aria-label="Settings sections" className="mb-6 flex gap-1 border-b border-line">
        {TABS.map((t) => {
          const selected = t.id === active.id;
          return (
            <button
              key={t.id}
              role="tab"
              type="button"
              aria-selected={selected}
              onClick={() => setParams({ tab: t.id }, { replace: true })}
              className={clsx(
                "-mb-px border-b-2 px-3 pb-2.5 pt-1 text-[13.5px] font-medium",
                selected ? "border-accent-fill text-fg" : "border-transparent text-muted hover:text-fg",
              )}
            >
              {t.label}
            </button>
          );
        })}
      </div>
      <div role="tabpanel">{active.el}</div>
    </>
  );
}
