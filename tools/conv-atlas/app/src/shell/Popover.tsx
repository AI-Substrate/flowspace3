import { type ReactNode, type RefObject, useEffect, useLayoutEffect, useRef, useState } from "react";

/**
 * Anchored popover: fixed-position so no overflow container clips it. Closes on
 * Escape (focus returns to the anchor) and on a pointer press outside.
 */
export function Popover({
  anchor,
  open,
  onClose,
  children,
  label,
  width = 280,
}: {
  anchor: RefObject<HTMLElement | null>;
  open: boolean;
  onClose: () => void;
  children: ReactNode;
  label: string;
  width?: number;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number; maxHeight: number } | null>(null);

  useLayoutEffect(() => {
    if (!open || !anchor.current) return;
    const r = anchor.current.getBoundingClientRect();
    const left = Math.min(r.left, innerWidth - width - 8);
    const below = innerHeight - r.bottom - 12;
    const above = r.top - 12;
    setPos(below >= 220 || below >= above ? { left, top: r.bottom + 6, maxHeight: below } : { left, top: Math.max(8, r.top - 6 - Math.min(above, 420)), maxHeight: Math.min(above, 420) });
  }, [open, anchor, width]);

  useEffect(() => {
    if (!open) return;
    const onDown = (e: PointerEvent) => {
      const t = e.target as Node;
      if (!ref.current?.contains(t) && !anchor.current?.contains(t)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
        anchor.current?.focus();
      }
    };
    document.addEventListener("pointerdown", onDown, true);
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("pointerdown", onDown, true);
      document.removeEventListener("keydown", onKey, true);
    };
  }, [open, onClose, anchor]);

  useEffect(() => {
    if (open && pos) ref.current?.querySelector<HTMLElement>("input, button, [tabindex]")?.focus();
  }, [open, pos]);

  if (!open) return null;
  return (
    <div
      ref={ref}
      className="sh-popover"
      role="dialog"
      aria-label={label}
      style={pos ? { left: pos.left, top: pos.top, width, maxHeight: Math.min(pos.maxHeight, 420) } : { visibility: "hidden" }}
    >
      {children}
    </div>
  );
}
