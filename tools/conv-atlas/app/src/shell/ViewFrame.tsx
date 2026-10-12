/**
 * Frame for the active view: Suspense for lazily loaded view chunks, and an
 * error boundary so one broken view (or a chunk missing after a rebuild)
 * never takes down the shell.
 */
import { Component, type ReactNode, Suspense } from "react";
import { EmptyState } from "../design/primitives";

/** Quiet placeholder: a header-height bar and the view name, faded in late so fast loads never flash. */
function StageSkeleton({ title }: { title: string }) {
  return (
    <div className="sh-stageskel" aria-busy="true" aria-label={`Loading ${title}`}>
      <div className="sh-stageskel__head">
        <span style={{ width: 96 }} />
        <span style={{ width: 220 }} />
      </div>
      <p className="sh-stageskel__label">Loading {title}</p>
    </div>
  );
}

class ViewBoundary extends Component<{ title: string; children: ReactNode }, { error: Error | null }> {
  state = { error: null as Error | null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    const chunk = /dynamically imported module|Importing a module script failed|Failed to fetch/i.test(error.message);
    return (
      <EmptyState
        title={`${this.props.title} failed to ${chunk ? "load" : "draw"}`}
        tone="error"
        action={
          <button type="button" className="ca-btn" onClick={() => (chunk ? location.reload() : this.setState({ error: null }))}>
            {chunk ? "Reload the page" : "Try again"}
          </button>
        }
      >
        <p className="mono">{error.message}</p>
        <p>{chunk ? "The view's code could not be fetched: the app was rebuilt since this page opened, or the server is unreachable. Reload to try again; your view, selection and filters are kept in the link." : "The other views still work: switch with G and a view key."}</p>
      </EmptyState>
    );
  }
}

/** Keyed by view id at the call site, so switching views resets a failed boundary. */
export function ViewFrame({ title, children }: { title: string; children: ReactNode }) {
  return (
    <ViewBoundary title={title}>
      <Suspense fallback={<StageSkeleton title={title} />}>{children}</Suspense>
    </ViewBoundary>
  );
}
