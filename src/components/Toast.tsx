import { createContext, useCallback, useContext, useRef, useState, type ReactNode } from "react";
import { CircleCheck, CircleAlert, Download } from "lucide-react";
import clsx from "clsx";

type Tone = "success" | "error" | "info";
export interface ToastAction {
  label: string;
  onClick: () => void;
}
interface ToastItem {
  id: number;
  tone: Tone;
  message: string;
  action?: ToastAction;
  leaving: boolean;
}

const ToastContext = createContext<(message: string, tone?: Tone, action?: ToastAction) => void>(() => {});

/**
 * Completion/status feedback (apple-design §16). Toasts rise from the bottom
 * and leave the same way, using transitions (not keyframes) so rapid saves
 * retarget smoothly. A toast with an action stays longer so it can be used.
 */
export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<ToastItem[]>([]);
  const next = useRef(1);

  const show = useCallback((message: string, tone: Tone = "success", action?: ToastAction) => {
    const id = next.current++;
    // Info toasts (e.g. "update available") carry a choice, so they stay longest.
    const life = action ? (tone === "info" ? 12000 : 7000) : 3200;
    setItems((xs) => [...xs.slice(-2), { id, tone, message, action, leaving: false }]);
    window.setTimeout(() => setItems((xs) => xs.map((t) => (t.id === id ? { ...t, leaving: true } : t))), life);
    window.setTimeout(() => setItems((xs) => xs.filter((t) => t.id !== id)), life + 300);
  }, []);

  return (
    <ToastContext.Provider value={show}>
      {children}
      <div aria-live="polite" className="pointer-events-none fixed bottom-6 left-1/2 z-50 flex -translate-x-1/2 flex-col items-center gap-2">
        {items.map((t) => (
          <div
            key={t.id}
            data-motion="slide"
            className={clsx(
              "toast flex items-center gap-2 rounded-full border border-line bg-surface px-4 py-2.5 text-[13px] font-medium shadow-raised",
              t.action && "pointer-events-auto pr-2",
              t.leaving && "toast-leaving",
            )}
          >
            {t.tone === "success" ? (
              <CircleCheck size={16} className="text-success" aria-hidden />
            ) : t.tone === "info" ? (
              <Download size={16} className="text-accent" aria-hidden />
            ) : (
              <CircleAlert size={16} className="text-danger" aria-hidden />
            )}
            {t.message}
            {t.action && (
              <button
                type="button"
                className="pressable ml-1 rounded-full bg-accent-soft px-3 py-1 text-[12.5px] font-semibold text-accent hover:bg-accent-soft/70"
                onClick={() => {
                  t.action?.onClick();
                  setItems((xs) => xs.map((x) => (x.id === t.id ? { ...x, leaving: true } : x)));
                }}
              >
                {t.action.label}
              </button>
            )}
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
}

export function useToast() {
  return useContext(ToastContext);
}
