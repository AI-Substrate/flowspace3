import type { ReactNode } from "react";

export interface ViewHeaderProps {
  title: string;
  /** one line of live context: "1,204 of 1,512 conversations, up to 3 Sep" */
  summary?: ReactNode;
  /** view-specific controls (segmented controls, toggles), right-aligned */
  children?: ReactNode;
}

/** The strip at the top of every view: name, live summary, view controls. */
export function ViewHeader({ title, summary, children }: ViewHeaderProps) {
  return (
    <header className="ca-viewhead">
      <div className="ca-viewhead__id">
        <h2 className="ca-viewhead__title">{title}</h2>
        {summary ? <p className="ca-viewhead__summary">{summary}</p> : null}
      </div>
      {children ? <div className="ca-viewhead__controls">{children}</div> : null}
    </header>
  );
}
