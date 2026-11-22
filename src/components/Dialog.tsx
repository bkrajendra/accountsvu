import { useEffect, useRef, type ReactNode } from "react";
import { X } from "lucide-react";

/**
 * Modal built on the native <dialog> element: focus trapping, Esc to close
 * and inert background come from the platform. It enters with a short scale
 * + fade (modals stay centred; the scrim dims to focus — apple-design §12).
 */
export function Dialog({
  open,
  onClose,
  title,
  description,
  children,
  footer,
  width = 520,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  description?: string;
  children: ReactNode;
  footer?: ReactNode;
  width?: number;
}) {
  const ref = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    const d = ref.current;
    if (!d) return;
    if (open && !d.open) {
      d.showModal();
      // showModal() moves focus to the first control; React's autoFocus ran before it.
      d.querySelector<HTMLElement>("[data-autofocus]")?.focus();
    }
    if (!open && d.open) d.close();
  }, [open]);

  return (
    <dialog
      ref={ref}
      onClose={onClose}
      onCancel={(e) => {
        e.preventDefault();
        onClose();
      }}
      onMouseDown={(e) => {
        if (e.target === ref.current) onClose(); // click on the scrim
      }}
      className="dialog m-auto rounded-2xl border border-line bg-surface p-0 text-fg shadow-raised"
      style={{ width: `min(${width}px, calc(100vw - 48px))` }}
    >
      {open && (
        <div className="flex max-h-[calc(100vh-96px)] flex-col">
          <header className="flex items-start justify-between gap-4 px-6 pb-3 pt-5">
            <div>
              <h2 className="text-title">{title}</h2>
              {description && <p className="mt-1 text-[13px] text-muted">{description}</p>}
            </div>
            <button
              type="button"
              onClick={onClose}
              aria-label="Close"
              className="pressable -mr-1.5 grid size-8 place-items-center rounded-lg text-muted hover:bg-hover hover:text-fg"
            >
              <X size={17} />
            </button>
          </header>
          <div className="overflow-y-auto px-6 pb-5 pt-2">{children}</div>
          {footer && (
            <footer className="flex justify-end gap-2 border-t border-line bg-surface-2 px-6 py-3.5">{footer}</footer>
          )}
        </div>
      )}
    </dialog>
  );
}
