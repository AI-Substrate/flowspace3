import { type ReactNode, useLayoutEffect, useRef, useState } from "react";

export interface TooltipProps {
  /** pointer position in viewport (client) pixels; null hides the tooltip */
  at: { x: number; y: number } | null;
  children: ReactNode;
}

const GAP = 14;

/**
 * Pointer-following tooltip for canvas/WebGL marks. Fixed-position so stage
 * overflow never clips it; flips to stay inside the viewport. Not focusable:
 * keyboard users get the same facts from the detail drawer.
 */
export function Tooltip({ at, children }: TooltipProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  useLayoutEffect(() => {
    const el = ref.current;
    if (!at || !el) {
      setPos(null);
      return;
    }
    const { width, height } = el.getBoundingClientRect();
    const left = at.x + GAP + width > innerWidth ? at.x - GAP - width : at.x + GAP;
    const top = at.y + GAP + height > innerHeight ? at.y - GAP - height : at.y + GAP;
    setPos({ left: Math.max(4, left), top: Math.max(4, top) });
  }, [at]);
  if (!at) return null;
  return (
    <div
      ref={ref}
      className="ca-tooltip"
      role="tooltip"
      style={pos ? { left: pos.left, top: pos.top } : { left: at.x + GAP, top: at.y + GAP, visibility: "hidden" }}
    >
      {children}
    </div>
  );
}
