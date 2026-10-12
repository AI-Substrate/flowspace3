import type { ReactNode } from "react";
import type { RGBA } from "../palette";
import { Swatch } from "./Swatch";

export interface ChipProps {
  children: ReactNode;
  /** toggled on: filled with the accent tint and announced as pressed */
  selected?: boolean;
  /** a category colour shown as a leading dot */
  color?: RGBA;
  /** a live count after the label (tabular numerals) */
  count?: number;
  onClick?: () => void;
  /** adds a trailing remove button labelled "Remove <title>" */
  onRemove?: () => void;
  title?: string;
  disabled?: boolean;
}

/**
 * A compact token for filters, tags and facets. With `onClick` it is a toggle
 * button (aria-pressed); without, a static label.
 */
export function Chip({ children, selected, color, count, onClick, onRemove, title, disabled }: ChipProps) {
  const body = (
    <>
      {color ? <Swatch color={color} /> : null}
      <span className="ca-chip__label">{children}</span>
      {count !== undefined ? <span className="ca-chip__count">{count.toLocaleString()}</span> : null}
    </>
  );
  return (
    <span className="ca-chip" data-selected={selected || undefined} data-disabled={disabled || undefined}>
      {onClick ? (
        <button type="button" className="ca-chip__main" aria-pressed={!!selected} onClick={onClick} title={title} disabled={disabled}>
          {body}
        </button>
      ) : (
        <span className="ca-chip__main" title={title}>
          {body}
        </span>
      )}
      {onRemove ? (
        <button type="button" className="ca-chip__remove" onClick={onRemove} aria-label={`Remove ${title ?? (typeof children === "string" ? children : "filter")}`} disabled={disabled}>
          <svg viewBox="0 0 10 10" width="10" height="10" aria-hidden="true">
            <path d="M2 2l6 6M8 2l-6 6" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
          </svg>
        </button>
      ) : null}
    </span>
  );
}
