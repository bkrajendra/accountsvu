import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { createHashRouter, RouterProvider } from "react-router";
import { AppShell } from "./components/AppShell";
import { ThemeProvider } from "./lib/theme";
import { primaryNav } from "./lib/nav";
import { Overview } from "./pages/Overview";
import { Placeholder } from "./pages/Placeholder";
import { Settings } from "./pages/settings/Settings";
import { Parties } from "./pages/parties/Parties";
import { Items } from "./pages/items/Items";
import { Sales } from "./pages/sales/Sales";
import { SalesEditor } from "./pages/sales/SalesEditor";
import { SalesDocument } from "./pages/sales/SalesDocument";
import { Purchases } from "./pages/purchases/Purchases";
import { PurchaseEditor } from "./pages/purchases/PurchaseEditor";
import { PurchaseDocument } from "./pages/purchases/PurchaseDocument";
import { Banking } from "./pages/banking/Banking";
import { VoucherEditor } from "./pages/banking/VoucherEditor";
import { VoucherDocument } from "./pages/banking/VoucherDocument";
import { MoneyBook } from "./pages/banking/MoneyBook";
import { Reconcile } from "./pages/banking/Reconcile";
import { Reports } from "./pages/reports/Reports";
import { BalanceSheetPage, ProfitAndLossPage, TrialBalancePage } from "./pages/reports/Statements";
import { AgeingPage, GeneralLedgerPage, JournalPage, PartyLedgerPage } from "./pages/reports/Ledgers";
import { Setup } from "./pages/Setup";
import { ReturnCentre } from "./pages/gst/ReturnCentre";
import { Gstr1Page } from "./pages/gst/Gstr1Page";
import { ToastProvider } from "./components/Toast";
import { SessionGate } from "./lib/session";
import { UpdateHost, UpdaterProvider } from "./lib/updater";

const queryClient = new QueryClient({
  defaultOptions: {
    // Local database: data is only stale when we change it ourselves.
    queries: { staleTime: 30_000, refetchOnWindowFocus: false, retry: 1 },
  },
});

// Hash routing keeps deep links working from the bundled tauri:// origin.
const router = createHashRouter([
  { path: "/setup", element: <Setup /> },
  {
    path: "/",
    element: <AppShell />,
    children: [
      { index: true, element: <Overview /> },
      { path: "sales", element: <Sales /> },
      { path: "sales/new", element: <SalesEditor /> },
      { path: "sales/:id", element: <SalesDocument /> },
      { path: "sales/:id/edit", element: <SalesEditor /> },
      { path: "purchases", element: <Purchases /> },
      { path: "purchases/new", element: <PurchaseEditor /> },
      { path: "purchases/:id", element: <PurchaseDocument /> },
      { path: "purchases/:id/edit", element: <PurchaseEditor /> },
      { path: "parties", element: <Parties /> },
      { path: "items", element: <Items /> },
      { path: "banking", element: <Banking /> },
      { path: "banking/vouchers/new", element: <VoucherEditor /> },
      { path: "banking/vouchers/:id", element: <VoucherDocument /> },
      { path: "banking/accounts/:id", element: <MoneyBook /> },
      { path: "banking/accounts/:id/reconcile", element: <Reconcile /> },
      { path: "reports", element: <Reports /> },
      { path: "reports/trial-balance", element: <TrialBalancePage /> },
      { path: "reports/profit-and-loss", element: <ProfitAndLossPage /> },
      { path: "reports/balance-sheet", element: <BalanceSheetPage /> },
      { path: "reports/ledger", element: <GeneralLedgerPage /> },
      { path: "reports/ledger/:id", element: <GeneralLedgerPage /> },
      { path: "reports/party", element: <PartyLedgerPage /> },
      { path: "reports/party/:id", element: <PartyLedgerPage /> },
      { path: "reports/journal", element: <JournalPage /> },
      { path: "reports/ageing/:kind", element: <AgeingPage /> },
      { path: "gst", element: <ReturnCentre /> },
      { path: "gst/gstr1/:period", element: <Gstr1Page /> },
      ...primaryNav
        .filter((n) => !["/", "/sales", "/purchases", "/parties", "/items", "/banking", "/reports", "/gst"].includes(n.path))
        .map((n) => ({ path: n.path.slice(1), element: <Placeholder item={n} /> })),
      { path: "settings", element: <Settings /> },
    ],
  },
]);

export default function App() {
  return (
    <ThemeProvider>
      <QueryClientProvider client={queryClient}>
        <ToastProvider>
          <UpdaterProvider>
            <SessionGate>
              <RouterProvider router={router} />
              <UpdateHost />
            </SessionGate>
          </UpdaterProvider>
        </ToastProvider>
      </QueryClientProvider>
    </ThemeProvider>
  );
}
