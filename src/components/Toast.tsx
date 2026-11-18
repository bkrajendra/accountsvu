import { createContext, useCallback, useContext, useRef, useState, type ReactNode } from "react";
import { CircleCheck, CircleAlert } from "lucide-react";
import clsx from "clsx";

type Tone = "success" | "error";
interface ToastItem {
  id: number;
  tone: Tone;
  message: string;
  leaving: boolean;
}

const ToastContext = createContext<(message: string, tone?: Tone) => void>(() => {});

/**
 * Completion/status feedback (apple-design §16). Toasts rise from the bottom
 * and leave the same way, using transitions (not keyframes) so rapid saves
 * retarget smoothly.
 */
export function ToastProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<ToastItem[]>([]);
  const next = useRef(1);

  const show = useCallback((message: string, tone: Tone = "success") => {
    const id = next.current++;
    setItems((xs) => [...xs.slice(-2), { id, tone, message, leaving: false }]);
    window.setTimeout(() => setItems((xs) => xs.map((t) => (t.id === id ? { ...t, leaving: true } : t))), 3200);
    window.setTimeout(() => setItems((xs) => xs.filter((t) => t.id !== id)), 3500);
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
              t.leaving && "toast-leaving",
            )}
          >
            {t.tone === "success" ? (
              <CircleCheck size={16} className="text-success" aria-hidden />
            ) : (
              <CircleAlert size={16} className="text-danger" aria-hidden />
            )}
            {t.message}
          </div>
        ))}
      </div>
    </ToastContext.Provider>
  );
}

export function useToast() {
  return useContext(ToastContext);
}
