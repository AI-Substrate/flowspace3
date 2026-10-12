/**
 * Keyboard: one global handler and the `?` overlay that documents it.
 * Keys never fire while typing in a field, and a handler that already
 * consumed the event (preventDefault) wins: the scrubber nudges itself.
 * Views switch with G then the view's own `hotkey` (ViewDef), so view letters
 * never collide with the single-key globals below.
 */
import { useEffect, useRef } from "react";
import { VIEWS } from "../core/registry";
import { DAY_MS } from "../core/semantics";
import { atlas } from "../core/store";
import { Kbd } from "../design/primitives";
import { getTheme, setTheme } from "../design/theme";
import { SPEEDS, togglePlay } from "./Transport";
import { useUi } from "./ui";

export const KEYS: { keys: string[]; what: string }[] = [
  { keys: ["⌘", "K"], what: "Search, jump and run commands" },
  { keys: ["/"], what: "Filter by text" },
  { keys: ["Space"], what: "Play or pause" },
  { keys: ["←", "→"], what: "Step the playhead one day (with Shift, one week)" },
  { keys: ["[", "]"], what: "Slower or faster playback" },
  { keys: ["Home", "End"], what: "Start or end of the playback range" },
  { keys: ["Tab"], what: "To the range handles: arrows move an edge a day (Shift: a week), Home and End go to its limits" },
  { keys: ["E"], what: "Show everything: jump to the end and stop" },
  { keys: ["L"], what: "Loop playback" },
  { keys: ["G", "…"], what: "Go to a view: G, then the view's key (listed below)" },
  { keys: ["R"], what: "Open the reader for the selected conversation" },
  { keys: ["I"], what: "Show or hide insights" },
  { keys: ["T"], what: "Switch between dark and light" },
  { keys: ["Esc"], what: "Close the panel, then clear the selection" },
  { keys: ["?"], what: "This list" },
];

function typing(t: EventTarget | null): boolean {
  const el = t as HTMLElement | null;
  return !!el && (el.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName));
}

/** How long G waits for the view key. */
const GO_WINDOW_MS = 1500;

export function useGlobalKeys(): void {
  useEffect(() => {
    let goTimer = 0;
    const endGo = () => {
      clearTimeout(goTimer);
      useUi.getState().setGo(false);
    };
    const onKey = (e: KeyboardEvent) => {
      const ui = useUi.getState();
      const s = atlas.getState();
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        ui.setPalette(!ui.paletteOpen);
        return;
      }
      if (e.defaultPrevented || e.metaKey || e.ctrlKey || e.altKey || typing(e.target) || ui.paletteOpen || ui.keysOpen) return;
      if (ui.go) {
        // second key of G+key: a view letter switches, anything else just cancels
        e.preventDefault();
        endGo();
        const view = VIEWS.find((v) => v.hotkey && v.hotkey.toLowerCase() === e.key.toLowerCase());
        if (view) s.setView(view.id);
        return;
      }
      if (e.key.toLowerCase() === "g" && !e.shiftKey) {
        e.preventDefault();
        ui.setGo(true);
        goTimer = window.setTimeout(endGo, GO_WINDOW_MS);
        return;
      }
      const t = s.time;
      const nudge = (days: number) => s.setTime({ cursorMs: Math.min(t.range[1], Math.max(t.range[0], t.cursorMs + days * DAY_MS)), playing: false });
      const speedStep = (dir: number) => {
        const i = SPEEDS.findIndex((x) => Number(x.value) >= t.speed);
        const next = SPEEDS[Math.min(SPEEDS.length - 1, Math.max(0, (i < 0 ? SPEEDS.length - 1 : i) + dir))];
        s.setTime({ speed: Number(next.value) });
      };
      const actions: Record<string, () => void> = {
        " ": togglePlay,
        ArrowLeft: () => nudge(e.shiftKey ? -7 : -1),
        ArrowRight: () => nudge(e.shiftKey ? 7 : 1),
        "[": () => speedStep(-1),
        "]": () => speedStep(1),
        Home: () => s.setTime({ cursorMs: t.range[0], playing: false }),
        End: () => s.setTime({ cursorMs: t.range[1], playing: false }),
        e: () => s.showAll(),
        l: () => s.setTime({ loop: !t.loop }),
        r: () => s.selected && ui.setPanel("reader"),
        i: () => ui.setPanel(ui.panel === "insights" ? null : "insights"),
        t: () => setTheme(getTheme() === "dark" ? "light" : "dark"),
        "?": () => ui.setKeys(true),
        "/": () => document.getElementById("sh-search")?.focus(),
        Escape: () => {
          if (ui.panel === "insights" || ui.panel === "reader") ui.setPanel(s.selected && ui.panel === "reader" ? "detail" : null);
          else if (s.selected) s.select(null);
          else if (s.picked.length) s.pick([]);
        },
      };
      const act = actions[e.key.length === 1 ? e.key.toLowerCase() : e.key] ?? actions[e.key];
      if (!act) return;
      if (e.key === " " && (e.target as HTMLElement).tagName === "BUTTON") return;
      e.preventDefault();
      act();
    };
    addEventListener("keydown", onKey);
    return () => {
      removeEventListener("keydown", onKey);
      endGo();
    };
  }, []);
}

export function KeysOverlay() {
  const open = useUi((s) => s.keysOpen);
  const setKeys = useUi((s) => s.setKeys);
  const ref = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const d = ref.current;
    if (open && d && !d.open) d.showModal();
    else if (!open && d?.open) d.close();
  }, [open]);
  return (
    <dialog ref={ref} className="sh-keys" aria-labelledby="sh-keys-title" onClose={() => setKeys(false)} onClick={(e) => e.target === ref.current && setKeys(false)}>
      <header className="sh-panelhead">
        <h2 id="sh-keys-title" className="sh-panelhead__title">
          Keyboard
        </h2>
        <button type="button" className="ca-btn" data-variant="ghost" onClick={() => setKeys(false)}>
          Close
        </button>
      </header>
      <dl className="sh-keys__list">
        {KEYS.map((k) => (
          <div key={k.what}>
            <dt>
              {k.keys.map((x) => (x === "…" ? <span key={x}> … </span> : <Kbd key={x}>{x}</Kbd>))}
            </dt>
            <dd>{k.what}</dd>
          </div>
        ))}
      </dl>
      <h3 className="sh-keys__sub">Views</h3>
      <dl className="sh-keys__list">
        {VIEWS.filter((v) => v.hotkey).map((v) => (
          <div key={v.id}>
            <dt>
              <Kbd>G</Kbd>
              <Kbd>{(v.hotkey as string).toUpperCase()}</Kbd>
            </dt>
            <dd>{v.title}</dd>
          </div>
        ))}
      </dl>
    </dialog>
  );
}
