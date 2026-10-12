import type { ReactNode } from "react";

export interface EmptyStateProps {
  /** what is missing, plainly: "No conversations match these filters" */
  title: string;
  /** why, and what to do about it */
  children?: ReactNode;
  /** one action that fixes it, e.g. a "Clear filters" button */
  action?: ReactNode;
  tone?: "neutral" | "error";
}

/** Fills a stage or panel when there is nothing to draw. Teaches the next step. */
export function EmptyState({ title, children, action, tone = "neutral" }: EmptyStateProps) {
  return (
    <div className="ca-empty" data-tone={tone} role={tone === "error" ? "alert" : "status"}>
      <p className="ca-empty__title">{title}</p>
      {children ? <div className="ca-empty__body">{children}</div> : null}
      {action ? <div className="ca-empty__action">{action}</div> : null}
    </div>
  );
}
