import { type KeyboardEvent, useRef } from "react";

export interface SegmentOption<T extends string> {
  value: T;
  label: string;
  /** tooltip and accessible description, e.g. what the mode does */
  title?: string;
}

export interface SegmentedControlProps<T extends string> {
  /** accessible name of the group, e.g. "Colour by" */
  label: string;
  value: T;
  options: SegmentOption<T>[];
  onChange: (value: T) => void;
  size?: "sm" | "md";
}

/** One-of-N switch (radio group). Arrow keys move and select; one tab stop. */
export function SegmentedControl<T extends string>({ label, value, options, onChange, size = "md" }: SegmentedControlProps<T>) {
  const ref = useRef<HTMLDivElement>(null);
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const step = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : e.key === "ArrowLeft" || e.key === "ArrowUp" ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    const i = options.findIndex((o) => o.value === value);
    const next = options[(i + step + options.length) % options.length];
    onChange(next.value);
    ref.current?.querySelector<HTMLButtonElement>(`[data-value="${CSS.escape(next.value)}"]`)?.focus();
  };
  return (
    <div ref={ref} className={`ca-seg ca-seg--${size}`} role="radiogroup" aria-label={label} onKeyDown={onKeyDown}>
      {options.map((o) => {
        const on = o.value === value;
        return (
          <button
            key={o.value}
            type="button"
            role="radio"
            aria-checked={on}
            tabIndex={on ? 0 : -1}
            data-value={o.value}
            title={o.title}
            className="ca-seg__item"
            onClick={() => onChange(o.value)}
          >
            {o.label}
          </button>
        );
      })}
    </div>
  );
}
