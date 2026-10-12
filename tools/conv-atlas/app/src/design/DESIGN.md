# conv-atlas design

## Direction
This is an instrument for an engineer to keep open all day on a second monitor, next to terminals. It works like an observatory control room at night:
- the chrome is graphite;
- the data is the only colour;
- one magenta cursor marks *now*: the playhead, the selection, the primary action.

Dark first. The light theme is a deliberate inversion with white ground and darker marks.

## Rules for view owners
- **Colours come only from `palette.ts`.** Never write a hex or `oklch()` in a view.
  - Marks: `categorical`, `communityColor`, `harnessColor`, `kindColor`, `roleColor`, `turnClassColor`, `ageRamp`.
  - Context: `dimmed()` for filtered or not-yet-existing marks (about 2:1 on the ground, always under every hue); `neutral(t)` for grids and edges.
  - Current/selected: `accent()`.
- Every palette function returns `[r,g,b,a]` floats 0..1 for the **current theme**.
  - WebGL/canvas views rebuild their colour buffers in `onThemeChange` (`theme.ts`).
  - DOM views use CSS variables (`var(--ink)`, `var(--muted)`, `var(--line)`, `var(--accent)`), which switch on their own.
- **Only the first 8 categorical colours are colour-blind-safe.** Past 8 categories, pair colour with a legend, hover and labels.
- **Light theme:** stroke point marks with `markStroke()`; on dark it is fully transparent. Some marks are under 4.5:1 on white.
- **Chrome comes from `primitives/`:**
  - `ViewHeader` (title, live summary, controls)
  - `Legend` (rows can toggle filters)
  - `Tooltip` (pointer-following, never clipped)
  - `Chip`, `SegmentedControl`, `EmptyState`, `Kbd`, `Swatch`
  - `.ca-btn` with `data-variant="primary|ghost"` and `data-size="icon"`; a toggle sets `aria-pressed`, and its pressed state looks like a selected chip
- **Type:** Recursive variable, one family with two voices.
  - UI text inherits the sans.
  - Wrap values that are code (guids, `conv:` addresses, raw timestamps) in `.mono` or `<code>`.
  - Counts use tabular numerals; that is the body default.
- **Motion:** use `--dur-*` and `--ease-out`, and animate state changes only. Reduced motion zeroes the durations. Imperative tweens must check `matchMedia("(prefers-reduced-motion: reduce)")` too.
- **Z-index:** use the `--z-*` scale only.
