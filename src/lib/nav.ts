import {
  BarChart3,
  FileCheck2,
  Landmark,
  LayoutGrid,
  Package,
  Receipt,
  Settings,
  ShoppingBag,
  Users,
  type LucideIcon,
} from "lucide-react";

export interface NavItem {
  path: string;
  label: string;
  icon: LucideIcon;
  /** Shown on placeholder pages until the module ships. */
  summary: string;
  phase?: string;
}

/** Sidebar order follows spec §6.1. */
export const primaryNav: NavItem[] = [
  { path: "/", label: "Overview", icon: LayoutGrid, summary: "Your business at a glance." },
  {
    path: "/sales",
    label: "Sales",
    icon: Receipt,
    summary: "Tax invoices, credit notes and estimates, with live GST breakup and receipt tracking.",
  },
  {
    path: "/purchases",
    label: "Purchases",
    icon: ShoppingBag,
    summary: "Supplier bills, debit notes and expenses, with input tax credit review for every bill.",
  },
  {
    path: "/parties",
    label: "Parties",
    icon: Users,
    summary: "Customers and suppliers with GSTIN, addresses, payment terms and running balances.",
  },
  {
    path: "/items",
    label: "Items",
    icon: Package,
    summary: "Goods and services with HSN/SAC codes, units and default tax rates.",
  },
  {
    path: "/banking",
    label: "Banking",
    icon: Landmark,
    summary: "Receipts, payments, cash and bank books, and bank statement reconciliation.",
    phase: "Phase 1.5",
  },
  {
    path: "/reports",
    label: "Reports",
    icon: BarChart3,
    summary: "Registers, ledgers, trial balance, profit & loss and ageing — every figure drills down to its source.",
    phase: "Phase 1.6",
  },
  {
    path: "/gst",
    label: "GST Filing",
    icon: FileCheck2,
    summary: "Prepare, reconcile and validate GSTR-1 and GSTR-3B, then record filing with ARN evidence.",
    phase: "Phase 2–3",
  },
];

export const settingsNav: NavItem = {
  path: "/settings",
  label: "Settings",
  icon: Settings,
  summary: "Business profile, numbering, tax defaults, appearance and backups.",
};
