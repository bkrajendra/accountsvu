import clsx from "clsx";

/** The folded-A brand mark (docs/brand-kit). Decorative: the wordmark or a
 *  label next to it carries the name for assistive tech. */
export function BrandMark({ size = 28, className }: { size?: number; className?: string }) {
  return (
    <img
      src="/brand/mark.png"
      alt=""
      aria-hidden
      width={size}
      height={size}
      draggable={false}
      className={clsx("shrink-0 select-none", className)}
    />
  );
}

/** "Accounts" in the text colour, "VU" in brand blue. */
export function Wordmark({ className }: { className?: string }) {
  return (
    <div className={clsx("text-[15px] font-semibold tracking-[-0.02em]", className)}>
      Accounts<span className="text-brand">VU</span>
    </div>
  );
}
