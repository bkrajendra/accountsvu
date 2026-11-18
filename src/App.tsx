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
import { Setup } from "./pages/Setup";
import { ToastProvider } from "./components/Toast";

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
      ...primaryNav
        .filter((n) => !["/", "/sales", "/purchases", "/parties", "/items", "/banking"].includes(n.path))
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
          <RouterProvider router={router} />
        </ToastProvider>
      </QueryClientProvider>
    </ThemeProvider>
  );
}
