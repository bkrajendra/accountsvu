import { Link } from "react-router";
import { BookOpenText, Clock, FileStack, Hourglass, Landmark, ListChecks, Scale, TrendingUp, type LucideIcon } from "lucide-react";
import { Card, PageHeader } from "../../components/ui";

const REPORTS: { to: string; title: string; body: string; icon: LucideIcon }[] = [
  { to: "/reports/profit-and-loss", title: "Profit & loss", body: "Sales, cost of sales, other income and expenses for a period.", icon: TrendingUp },
  { to: "/reports/balance-sheet", title: "Balance sheet", body: "What the business owns and owes on a date.", icon: Landmark },
  { to: "/reports/trial-balance", title: "Trial balance", body: "Every ledger's opening, movement and closing balance.", icon: Scale },
  { to: "/reports/ledger", title: "General ledger", body: "Every entry through one account, with running balance.", icon: BookOpenText },
  { to: "/reports/journal", title: "Journal register", body: "Every posting with its lines, in date order.", icon: ListChecks },
  { to: "/reports/ageing/receivables", title: "Receivables ageing", body: "Who owes you, and for how long.", icon: Hourglass },
  { to: "/reports/ageing/payables", title: "Payables ageing", body: "What you owe suppliers, and how overdue.", icon: Clock },
  { to: "/reports/party", title: "Party statement", body: "A customer's or supplier's account with you.", icon: FileStack },
];

export function Reports() {
  return (
    <div>
      <PageHeader title="Reports" subtitle="Every figure opens the ledger behind it, and every ledger line opens its document." />
      <div className="grid grid-cols-2 gap-4 xl:grid-cols-4">
        {REPORTS.map((r) => (
          <Link key={r.to} to={r.to} className="group rounded-2xl focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-fill">
            <Card className="h-full p-5 transition-colors duration-150 group-hover:bg-surface-2">
              <span className="mb-3 grid size-10 place-items-center rounded-xl bg-accent-soft text-accent">
                <r.icon size={19} />
              </span>
              <h2 className="text-[15px] font-semibold">{r.title}</h2>
              <p className="mt-1 text-[13px] text-muted">{r.body}</p>
            </Card>
          </Link>
        ))}
      </div>
    </div>
  );
}
