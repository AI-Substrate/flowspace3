/** The shell's icon set: 16px, 1.5px stroke, currentColor. One style everywhere. */
import type { ReactNode } from "react";

function Icon({ children, size = 16 }: { children: ReactNode; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      {children}
    </svg>
  );
}

export const Play = () => (
  <Icon>
    <path d="M5 3.5v9l7-4.5z" fill="currentColor" stroke="none" />
  </Icon>
);
export const Pause = () => (
  <Icon>
    <path d="M5.5 3.5v9M10.5 3.5v9" strokeWidth="2" />
  </Icon>
);
export const Loop = () => (
  <Icon>
    <path d="M3 7.5V7a3 3 0 0 1 3-3h6.5M10.5 2l2 2-2 2M13 8.5V9a3 3 0 0 1-3 3H3.5M5.5 14l-2-2 2-2" />
  </Icon>
);
export const ToEnd = () => (
  <Icon>
    <path d="M3.5 3.5l6 4.5-6 4.5zM12.5 3.5v9" />
  </Icon>
);
export const Sun = () => (
  <Icon>
    <circle cx="8" cy="8" r="3" />
    <path d="M8 1.5v1.5M8 13v1.5M1.5 8H3M13 8h1.5M3.4 3.4l1 1M11.6 11.6l1 1M3.4 12.6l1-1M11.6 4.4l1-1" />
  </Icon>
);
export const Moon = () => (
  <Icon>
    <path d="M13 9.5A5.5 5.5 0 0 1 6.5 3a5.5 5.5 0 1 0 6.5 6.5z" />
  </Icon>
);
export const Search = () => (
  <Icon>
    <circle cx="7" cy="7" r="4.5" />
    <path d="M10.5 10.5L14 14" />
  </Icon>
);
export const Close = () => (
  <Icon>
    <path d="M4 4l8 8M12 4l-8 8" />
  </Icon>
);
export const Copy = () => (
  <Icon>
    <rect x="5.5" y="5.5" width="8" height="8" rx="1.5" />
    <path d="M10.5 3.5v-.5A1.5 1.5 0 0 0 9 1.5H3A1.5 1.5 0 0 0 1.5 3v6A1.5 1.5 0 0 0 3 10.5h.5" />
  </Icon>
);
export const Chevron = () => (
  <Icon size={12}>
    <path d="M4 6l4 4 4-4" />
  </Icon>
);
export const Insight = () => (
  <Icon>
    <path d="M8 1.5a4.5 4.5 0 0 0-2.5 8.2V11h5V9.7A4.5 4.5 0 0 0 8 1.5zM6 13.5h4" />
  </Icon>
);
export const Reader = () => (
  <Icon>
    <path d="M2.5 3.5h11M2.5 6.5h11M2.5 9.5h7M2.5 12.5h9" />
  </Icon>
);
export const Back = () => (
  <Icon>
    <path d="M10 3.5L5.5 8l4.5 4.5" />
  </Icon>
);
