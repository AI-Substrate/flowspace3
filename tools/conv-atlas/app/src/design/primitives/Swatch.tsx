import { css, type RGBA } from "../palette";

/** A colour key: dot for points, square for areas, line for edges. Decorative to screen readers. */
export function Swatch({ color, shape = "dot" }: { color: RGBA; shape?: "dot" | "square" | "line" }) {
  return <span className={`ca-swatch ca-swatch--${shape}`} style={{ background: css(color) }} aria-hidden="true" />;
}
