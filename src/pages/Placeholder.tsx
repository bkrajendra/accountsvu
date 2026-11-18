import { Sparkles } from "lucide-react";
import type { NavItem } from "../lib/nav";
import { Badge, Card, PageHeader } from "../components/ui";

/** Empty state for modules not built yet — says what's coming and when. */
export function Placeholder({ item }: { item: NavItem }) {
  const Icon = item.icon;
  return (
    <>
      <PageHeader title={item.label} />
      <Card className="flex flex-col items-center px-8 py-16 text-center">
        <div className="grid size-14 place-items-center rounded-2xl bg-accent-soft text-accent">
          <Icon size={26} strokeWidth={1.7} />
        </div>
        <h2 className="text-title mt-5">{item.label} is on the way</h2>
        <p className="mt-1.5 max-w-[440px] text-muted">{item.summary}</p>
        {item.phase && (
          <div className="mt-5">
            <Badge tone="accent" icon={<Sparkles size={11} />}>
              Planned for {item.phase}
            </Badge>
          </div>
        )}
      </Card>
    </>
  );
}
