import { useState } from "react";
import type { RGBA } from "../palette";
import { Swatch } from "./Swatch";

export interface LegendItem {
  /** stable key; with onToggle, usually the filter key */
  key: string;
  label: string;
  color: RGBA;
  count?: number;
  /** false = filtered out; rendered faint */
  active?: boolean;
  /** not a filter value (e.g. "none", "unknown"): a plain row even when the legend toggles */
  fixed?: boolean;
}

export interface LegendProps {
  /** what the colour encodes, e.g. "Community" */
  title: string;
  items: LegendItem[];
  shape?: "dot" | "square" | "line";
  /** makes rows toggle buttons (e.g. filter by this category) */
  onToggle?: (key: string) => void;
  /** rows beyond this collapse behind an "N more" button */
  max?: number;
}

/** Categorical key for a view. Sits in a stage corner; rows can toggle filters. */
export function Legend({ title, items, shape = "dot", onToggle, max = 12 }: LegendProps) {
  const [expanded, setExpanded] = useState(false);
  const shown = expanded ? items : items.slice(0, max);
  const overflow = items.length - Math.min(items.length, max);
  return (
    <section className="ca-legend" aria-label={`${title} legend`}>
      <h3 className="ca-legend__title">{title}</h3>
      <ul className="ca-legend__list">
        {shown.map((it) => {
          const row = (
            <>
              <Swatch color={it.color} shape={shape} />
              <span className="ca-legend__label">{it.label}</span>
              {it.count !== undefined ? <span className="ca-legend__count">{it.count.toLocaleString()}</span> : null}
            </>
          );
          return (
            <li key={it.key} className="ca-legend__row" data-inactive={it.active === false || undefined}>
              {onToggle && !it.fixed ? (
                <button type="button" aria-pressed={it.active !== false} onClick={() => onToggle(it.key)}>
                  {row}
                </button>
              ) : (
                row
              )}
            </li>
          );
        })}
      </ul>
      {overflow > 0 ? (
        <button type="button" className="ca-legend__more" aria-expanded={expanded} onClick={() => setExpanded(!expanded)}>
          {expanded ? "Show fewer" : `${overflow} more`}
        </button>
      ) : null}
    </section>
  );
}
