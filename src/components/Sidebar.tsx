import { NavLink } from "react-router";
import clsx from "clsx";
import { PanelLeftClose, PanelLeftOpen } from "lucide-react";
import { primaryNav, settingsNav, type NavItem } from "../lib/nav";

interface SidebarProps {
  collapsed: boolean;
  onToggle: () => void;
}

/**
 * Persistent, collapsible sidebar (spec §6.1). A heavier translucent
 * material separates it structurally from content (apple-design §12).
 * Collapse is instant: it's a frequent, keyboard-bound toggle (Ctrl+B).
 */
export function Sidebar({ collapsed, onToggle }: SidebarProps) {
  return (
    <aside
      className={clsx(
        "material flex shrink-0 flex-col border-r border-line",
        collapsed ? "w-[64px]" : "w-[232px]",
      )}
      aria-label="Main navigation"
    >
      <div className={clsx("flex h-14 items-center", collapsed ? "justify-center" : "gap-2.5 px-4")}>
        <Logo />
        {!collapsed && (
          <div className="leading-tight">
            <div className="text-[15px] font-semibold tracking-[-0.015em]">AccountsVU</div>
            <div className="text-[11px] font-medium text-muted">Books &amp; GST</div>
          </div>
        )}
      </div>

      <nav className="flex flex-1 flex-col gap-0.5 overflow-y-auto px-2.5 pt-2">
        {primaryNav.map((item) => (
          <SidebarLink key={item.path} item={item} collapsed={collapsed} />
        ))}
      </nav>

      <div className="flex flex-col gap-0.5 border-t border-line px-2.5 py-2.5">
        <SidebarLink item={settingsNav} collapsed={collapsed} />
        <button
          type="button"
          onClick={onToggle}
          title={collapsed ? "Expand sidebar (Ctrl+B)" : "Collapse sidebar (Ctrl+B)"}
          aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"}
          className={clsx(
            "pressable flex h-9 items-center gap-3 rounded-lg text-[13px] font-medium text-muted hover:bg-hover hover:text-fg",
            collapsed ? "justify-center" : "px-2.5",
          )}
        >
          {collapsed ? <PanelLeftOpen size={18} strokeWidth={1.8} /> : <PanelLeftClose size={18} strokeWidth={1.8} />}
          {!collapsed && <span>Collapse</span>}
        </button>
      </div>
    </aside>
  );
}

function SidebarLink({ item, collapsed }: { item: NavItem; collapsed: boolean }) {
  const Icon = item.icon;
  return (
    <NavLink
      to={item.path}
      end={item.path === "/"}
      title={collapsed ? item.label : undefined}
      className={({ isActive }) =>
        clsx(
          "pressable group flex h-9 items-center gap-3 rounded-lg text-[13.5px] font-medium",
          collapsed ? "justify-center" : "px-2.5",
          isActive ? "bg-accent-soft text-accent" : "text-fg/85 hover:bg-hover hover:text-fg",
        )
      }
    >
      {({ isActive }) => (
        <>
          <Icon size={18} strokeWidth={isActive ? 2.1 : 1.8} className={isActive ? "" : "text-muted group-hover:text-fg"} />
          {!collapsed && <span className="truncate">{item.label}</span>}
        </>
      )}
    </NavLink>
  );
}

function Logo() {
  return (
    <div
      aria-hidden
      className="grid size-8 shrink-0 place-items-center rounded-[9px] bg-gradient-to-b from-[#2b8cff] to-[#0062d1] text-white shadow-[inset_0_1px_0_rgba(255,255,255,0.25),0_1px_2px_rgba(0,0,0,0.2)]"
    >
      <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round">
        <path d="M4 6l8 13 8-13" />
        <path d="M8.5 6h7" opacity="0.7" />
      </svg>
    </div>
  );
}
