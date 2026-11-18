import type { ReactNode } from "react";

/** Two-column settings row: title/description on the left, content right. */
export function SettingsSection({
  title,
  description,
  actions,
  children,
}: {
  title: string;
  description?: string;
  actions?: ReactNode;
  children: ReactNode;
}) {
  return (
    <section className="mb-8 grid gap-4 lg:grid-cols-[220px_1fr] lg:gap-8">
      <div>
        <h2 className="text-title">{title}</h2>
        {description && <p className="mt-1 text-[13px] text-muted">{description}</p>}
        {actions && <div className="mt-3">{actions}</div>}
      </div>
      <div className="min-w-0">{children}</div>
    </section>
  );
}
