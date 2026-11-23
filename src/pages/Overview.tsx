import type { CSSProperties, ReactNode } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "react-router";
import {
  AlertTriangle,
  ArrowRight,
  Building2,
  CalendarClock,
  CircleCheck,
  FileWarning,
  HardDriveDownload,
  Info,
  Landmark,
  Receipt,
  ShoppingBag,
  Users,
  type LucideIcon,
} from "lucide-react";
import clsx from "clsx";
import { api, keys, type MoneyString, type ReturnPeriod } from "../lib/api";
import { formatInr, greeting, monthBounds, monthLabel } from "../lib/format";
import { shortDate } from "../lib/sales";
import { Badge, Button, Card, PageHeader } from "../components/ui";

// The entrance plays once per app launch — the overview is revisited often,
// so repeat visits render instantly (animate skill: frequency gate).
let hasAnimated = false;

export function Overview() {
  const { start, end } = monthBounds();
  const { data, isLoading } = useQuery({
    queryKey: ["dashboard", start, end],
    queryFn: () => api.dashboardSummary(start, end),
  });

  const navigate = useNavigate();
  const { data: profile } = useQuery({ queryKey: keys.profile, queryFn: api.businessProfile });
  const reg = profile?.registrations.find((r) => r.status === "active");
  const animate = !hasAnimated;
  hasAnimated = true;
  const configured = data?.organisationConfigured ?? true;
  const { data: backup } = useQuery({ queryKey: keys.backupStatus, queryFn: api.backupStatus, enabled: configured });
  const { data: books } = useQuery({ queryKey: keys.booksControl, queryFn: api.booksControl, enabled: configured });
  const { data: centre } = useQuery({
    queryKey: keys.report("gst", "centre", "", ""),
    queryFn: () => api.returnCentre(null, null),
    enabled: configured && !!reg,
  });
  const booksStart = profile?.organisation?.booksStartDate ?? "";
  const returns = nextReturns(centre?.periods ?? [], booksStart);
  const twoB = twoBStatus(centre?.periods ?? [], booksStart);

  const cards: { label: string; value?: MoneyString; icon: LucideIcon; hint: string; estimate?: boolean }[] = [
    { label: "Sales this month", value: data?.salesThisMonth, icon: Receipt, hint: monthLabel() },
    { label: "Purchases this month", value: data?.purchasesThisMonth, icon: ShoppingBag, hint: monthLabel() },
    { label: "Cash & bank", value: data?.cashAndBank, icon: Landmark, hint: "All accounts" },
    { label: "Customer dues", value: data?.customerDues, icon: Users, hint: "Outstanding receivables" },
    {
      label: "GST payable",
      value: data?.estimatedGstPayable,
      icon: FileWarning,
      hint: "From books",
      estimate: true,
    },
  ];

  return (
    <>
      <PageHeader
        title={greeting()}
        subtitle={new Date().toLocaleDateString("en-IN", { weekday: "long", day: "numeric", month: "long", year: "numeric" })}
        actions={
          <>
            <Button onClick={() => navigate("/purchases/new")}>
              <ShoppingBag size={15} /> Record bill
            </Button>
            <Button variant="primary" onClick={() => navigate("/sales/new")}>
              <Receipt size={15} /> New invoice
            </Button>
          </>
        }
      />

      {!configured && (
        <Card className={clsx("mb-6 overflow-hidden", animate && "rise-in")}>
          <div className="flex items-center gap-5 p-5">
            <div className="grid size-11 shrink-0 place-items-center rounded-xl bg-accent-soft text-accent">
              <Building2 size={22} strokeWidth={1.8} />
            </div>
            <div className="min-w-0 flex-1">
              <h2 className="text-title">Set up your business</h2>
              <p className="mt-0.5 text-muted">
                Add your legal name, GSTIN and financial year. It takes about two minutes, and you can change it later.
              </p>
            </div>
            <Button variant="primary" className="shrink-0" onClick={() => navigate("/setup")}>
              Get started <ArrowRight size={15} />
            </Button>
          </div>
        </Card>
      )}

      <div className="grid grid-cols-2 gap-4 lg:grid-cols-3 xl:grid-cols-5">
        {cards.map((c, i) => (
          <Card
            key={c.label}
            className={clsx("flex flex-col p-4", animate && "rise-in")}
            style={{ "--i": i } as CSSProperties}
          >
            <div className="flex items-center justify-between">
              <span className="text-[12.5px] font-medium text-muted">{c.label}</span>
              <c.icon size={16} className="text-subtle" aria-hidden />
            </div>
            <div className={clsx("num mt-3 text-[22px] font-semibold tracking-[-0.02em]", isLoading && "text-subtle")}>
              {c.value === undefined ? "—" : formatInr(c.value)}
            </div>
            <div className="mt-auto flex items-center gap-1.5 pt-2 text-[12px] text-muted">
              {c.estimate && <Badge tone="warning">Estimate</Badge>}
              <span className="truncate">{c.hint}</span>
            </div>
          </Card>
        ))}
      </div>

      <div className="mt-6 grid gap-4 lg:grid-cols-[1.6fr_1fr]">
        <Card className={clsx(animate && "rise-in")} style={{ "--i": 5 } as CSSProperties}>
          <div className="flex items-center justify-between border-b border-line px-5 py-3.5">
            <h2 className="text-title">Next actions</h2>
            <span className="text-[12px] text-muted">Exceptions first</span>
          </div>
          <ul className="divide-y divide-line">
            {!configured ? (
              <ActionRow
                icon={Building2}
                tone="accent"
                title="Complete business & GST profile"
                detail="Needed before you can issue tax invoices."
                status={<Badge tone="accent">To do</Badge>}
                onClick={() => navigate("/setup")}
              />
            ) : (
              !reg && (
                <ActionRow
                  icon={Building2}
                  tone="neutral"
                  title="Add your GSTIN when you register"
                  detail="You're set up as unregistered, so you issue bills of supply."
                  status={<Badge>Optional</Badge>}
                  onClick={() => navigate("/settings?tab=gst")}
                />
              )
            )}
            {!reg ? (
              <ActionRow
                icon={CalendarClock}
                tone="neutral"
                title="GSTR-1 and GSTR-3B"
                detail="Due dates appear once a GSTIN and filing frequency are set."
                status={<Badge>Not configured</Badge>}
              />
            ) : returns ? (
              <ActionRow
                icon={CalendarClock}
                tone={returns.overdue ? "warning" : "neutral"}
                title={`GSTR-1 and GSTR-3B for ${returns.period.label}`}
                detail={returns.detail}
                status={
                  returns.overdue ? (
                    <Badge tone="warning" icon={<AlertTriangle size={11} />}>Overdue</Badge>
                  ) : returns.period.status === "due" ? (
                    <Badge tone="accent">Due</Badge>
                  ) : (
                    <Badge>Open</Badge>
                  )
                }
                onClick={() => navigate(`/gst/${returns.gstr1Done ? "gstr3b" : "gstr1"}/${returns.period.period}`)}
              />
            ) : (
              centre && (
                <ActionRow
                  icon={CalendarClock}
                  tone="neutral"
                  title="GSTR-1 and GSTR-3B"
                  detail="Every return this year is recorded as filed."
                  status={<Badge tone="success">Done</Badge>}
                  onClick={() => navigate("/gst")}
                />
              )
            )}
            {reg && twoB && (
              <ActionRow
                icon={AlertTriangle}
                tone={twoB.review ? "warning" : "neutral"}
                title={twoB.title}
                detail={twoB.detail}
                status={
                  twoB.review ? (
                    <Badge tone="warning">{twoB.review} to review</Badge>
                  ) : twoB.imported ? (
                    <Badge tone="success">Done</Badge>
                  ) : (
                    <Badge tone="accent">To do</Badge>
                  )
                }
                onClick={() => navigate(`/gst/gstr2b/${twoB.period.period}`)}
              />
            )}
            {configured && backup && (
              <ActionRow
                icon={HardDriveDownload}
                tone={backup.due ? "warning" : "neutral"}
                title={backup.lastBackup ? (backup.due ? `Last backup ${backup.daysSinceBackup} days ago` : "Backed up") : "No backup yet"}
                detail={
                  backup.lastBackup
                    ? backup.due
                      ? "Make a fresh encrypted backup and keep it off this computer."
                      : `Last backup ${backup.daysSinceBackup === 0 ? "today" : backup.daysSinceBackup === 1 ? "yesterday" : `${backup.daysSinceBackup} days ago`}.`
                    : "Make an encrypted backup and keep it off this computer."
                }
                status={backup.due ? <Badge tone="warning" icon={<AlertTriangle size={11} />}>Attention</Badge> : <Badge tone="success">Done</Badge>}
                onClick={() => navigate("/settings?tab=security")}
              />
            )}
          </ul>
        </Card>

        <Card className={clsx("p-5", animate && "rise-in")} style={{ "--i": 6 } as CSSProperties}>
          <h2 className="text-title">Books health</h2>
          <ul className="mt-4 space-y-3.5 text-[13.5px]">
            <HealthRow ok label="Journals balance" />
            <HealthRow ok label="Posted entries are immutable" />
            <HealthRow ok label="Data stored on this computer" />
            {books?.lock ? <HealthRow ok label={`Books locked up to ${shortDate(books.lock.lockedUntil)}`} /> : <HealthRow label="Period lock not set" />}
          </ul>
          <p className="mt-5 flex gap-2 rounded-xl bg-surface-2 p-3 text-[12.5px] text-muted">
            <Info size={15} className="mt-px shrink-0 text-subtle" aria-hidden />
            Tax figures are estimates from your books until a return is filed with an ARN.
          </p>
        </Card>
      </div>
    </>
  );
}

function ActionRow({
  icon: Icon,
  tone,
  title,
  detail,
  status,
  onClick,
}: {
  icon: LucideIcon;
  tone: "accent" | "warning" | "neutral";
  title: string;
  detail: string;
  status: ReactNode;
  onClick?: () => void;
}) {
  return (
    <li>
      <button type="button" onClick={onClick} className="group flex w-full items-center gap-4 px-5 py-3.5 text-left transition-colors duration-150 hover:bg-hover">
        <span
          className={clsx(
            "grid size-9 shrink-0 place-items-center rounded-[10px]",
            tone === "accent" && "bg-accent-soft text-accent",
            tone === "warning" && "bg-warning-soft text-warning",
            tone === "neutral" && "bg-active text-muted",
          )}
        >
          <Icon size={17} strokeWidth={1.9} />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block font-medium">{title}</span>
          <span className="block truncate text-[12.5px] text-muted">{detail}</span>
        </span>
        {status}
        <ArrowRight size={15} className="text-subtle opacity-0 transition-opacity duration-150 group-hover:opacity-100" aria-hidden />
      </button>
    </li>
  );
}

function HealthRow({ ok = false, label }: { ok?: boolean; label: string }) {
  return (
    <li className="flex items-center gap-2.5">
      {ok ? (
        <CircleCheck size={17} className="text-success" aria-label="OK" />
      ) : (
        <AlertTriangle size={17} className="text-warning" aria-label="Needs attention" />
      )}
      <span className={ok ? "" : "text-muted"}>{label}</span>
    </li>
  );
}

/** Months that count: inside the books, or with something recorded. */
const relevant = (p: ReturnPeriod, booksStart: string) => p.to >= booksStart || p.documentCount > 0 || p.netItcPaise !== 0;

/** The oldest month with a return still to file, else the open month. */
function nextReturns(periods: ReturnPeriod[], booksStart: string) {
  const oldestFirst = [...periods].reverse().filter((p) => relevant(p, booksStart));
  const pending = oldestFirst.find((p) => p.status !== "open" && (!p.gstr1Filed || !p.gstr3bFiled));
  const period = pending ?? oldestFirst.find((p) => p.status === "open" && (!p.gstr1Filed || !p.gstr3bFiled));
  if (!period) return null;
  const gstr1Done = !!period.gstr1Filed;
  const parts = [
    gstr1Done ? "GSTR-1 filed" : `GSTR-1 due ${shortDate(period.gstr1Due)}`,
    period.gstr3bFiled ? "GSTR-3B filed" : `GSTR-3B due ${shortDate(period.gstr3bDue)}`,
  ];
  if (period.errorCount) parts.push(`${period.errorCount} to fix`);
  const overdue = (!gstr1Done && period.status === "past_due") || (!period.gstr3bFiled && period.gstr3bStatus === "past_due");
  return { period, gstr1Done, overdue, detail: parts.join(" · ") };
}

/** GSTR-2B: lines waiting for a decision, else the latest closed month without a statement. */
function twoBStatus(periods: ReturnPeriod[], booksStart: string) {
  const months = periods.filter((p) => relevant(p, booksStart));
  const review = months.find((p) => p.gstr2bNeedsReview > 0);
  if (review) {
    const total = months.reduce((a, p) => a + p.gstr2bNeedsReview, 0);
    return { period: review, review: total, imported: true, title: `GSTR-2B for ${review.label}`, detail: "Some lines don't match your purchases yet. Accept, defer or fix them." };
  }
  const closed = months.find((p) => p.status !== "open");
  if (!closed) return null;
  if (!closed.gstr2bImported)
    return { period: closed, review: 0, imported: false, title: `Import GSTR-2B for ${closed.label}`, detail: "Download it from the GST portal to reconcile input tax credit." };
  return { period: closed, review: 0, imported: true, title: `GSTR-2B for ${closed.label}`, detail: "Reconciled with your purchases." };
}
