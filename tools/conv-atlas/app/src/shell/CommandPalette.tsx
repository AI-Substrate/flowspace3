/**
 * ⌘K: jump to a conversation (label, guid, seat), switch view, colour-by,
 * filter, open an insight, or run an action. Keyboard-first combobox.
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { VIEWS } from "../core/registry";
import { NO_FILTERS } from "../core/semantics";
import { atlas, type ColorBy, useAtlas } from "../core/store";
import { categorical, communityColor, harnessColor, kindColor, type RGBA } from "../design/palette";
import { Kbd, Swatch } from "../design/primitives";
import { getTheme, setTheme } from "../design/theme";
import { copyText } from "./copy";
import { fmt } from "./format";
import { useTheme } from "./hooks";
import { togglePlay } from "./Transport";
import { useUi } from "./ui";

interface Command {
  id: string;
  group: string;
  label: string;
  hint?: string;
  color?: RGBA;
  /** lower-cased text the query matches against */
  hay: string;
  run: () => void;
}

const COLOR_BY: { value: ColorBy; label: string }[] = [
  { value: "community", label: "community" },
  { value: "repo", label: "repo" },
  { value: "harness", label: "harness" },
  { value: "kind", label: "kind" },
  { value: "age", label: "age" },
];
const MAX = 60;

function useCommands(): Command[] {
  const snapshot = useAtlas((s) => s.snapshot);
  const insights = useAtlas((s) => s.insights);
  const selected = useAtlas((s) => s.selected);
  // swatches and the theme command's label depend on the theme
  const theme = useTheme();
  return useMemo(() => {
    const s = atlas.getState();
    const ui = useUi.getState();
    const out: Command[] = [];
    const add = (c: Omit<Command, "hay"> & { hay?: string }) => out.push({ ...c, hay: (c.hay ?? `${c.group} ${c.label}`).toLowerCase() });

    VIEWS.forEach((v) => add({ id: `view:${v.id}`, group: "Views", label: `Go to ${v.title}`, hint: v.hotkey ? `G ${v.hotkey.toUpperCase()}` : undefined, hay: `view go ${v.title} ${v.description}`, run: () => s.setView(v.id) }));
    add({ id: "act:play", group: "Actions", label: "Play or pause", hint: "Space", run: togglePlay });
    add({ id: "act:showall", group: "Actions", label: "Show everything (jump to the end)", hint: "E", run: s.showAll });
    add({ id: "act:clear", group: "Actions", label: "Clear filters", run: () => s.setFilters(NO_FILTERS) });
    add({ id: "act:theme", group: "Actions", label: `Switch to ${getTheme() === "dark" ? "light" : "dark"} theme`, hint: "T", run: () => setTheme(getTheme() === "dark" ? "light" : "dark") });
    add({ id: "act:keys", group: "Actions", label: "Keyboard shortcuts", hint: "?", run: () => ui.setKeys(true) });
    add({ id: "act:insights", group: "Actions", label: "Show insights", hint: "I", run: () => ui.setPanel("insights") });
    if (selected) {
      add({ id: "act:reader", group: "Actions", label: "Open the reader for the selected conversation", hint: "R", run: () => ui.setPanel("reader") });
      add({ id: "act:copy", group: "Actions", label: "Copy fs3 command for the selected conversation", hay: "copy flowspace3 get fs3 address command", run: () => copyText(`flowspace3 get conv:${selected}`, "Copied the fs3 command") });
    }
    for (const c of COLOR_BY) add({ id: `color:${c.value}`, group: "Colour by", label: `Colour by ${c.label}`, hay: `colour color by ${c.label}`, run: () => s.setColorBy(c.value) });

    for (const x of insights?.insights ?? [])
      add({ id: `insight:${x.id}`, group: "Insights", label: x.title, hint: fmt.day(x.created_ms), color: kindColor(x.kind), hay: `insight ${x.kind} ${x.title}`, run: () => {
        ui.setPanel("insights");
        s.setActiveInsight(x.id);
      } });

    if (snapshot) {
      const { meta } = snapshot;
      meta.harnesses.forEach((h) => add({ id: `f:h:${h}`, group: "Filters", label: `Filter to harness ${h}`, color: harnessColor(h), run: () => s.setFilters({ harnesses: [h] }) }));
      meta.repos.forEach((r) => add({ id: `f:r:${r.key}`, group: "Filters", label: `Filter to repo ${r.label}`, color: categorical(snapshot.slots.repo.get(r.key) ?? 0), run: () => s.setFilters({ repos: [r.key] }) }));
      meta.seats.forEach((st) => add({ id: `f:s:${st.id}`, group: "Filters", label: `Filter to seat ${st.id}`, run: () => s.setFilters({ seats: [st.id] }) }));
      meta.communities.forEach((c, i) => add({ id: `f:c:${i}`, group: "Filters", label: `Filter to community ${c.label}`, color: communityColor(snapshot.slots.community.get(String(i)) ?? i), run: () => s.setFilters({ communities: [i] }) }));
      (["session", "subagent"] as const).forEach((k) => add({ id: `f:k:${k}`, group: "Filters", label: `Filter to ${k}s`, color: kindColor(k), run: () => s.setFilters({ kinds: [k] }) }));
      snapshot.conversations.forEach((c) =>
        add({
          id: `conv:${c.guid}`,
          group: "Conversations",
          label: c.label,
          hint: fmt.day(c.started_ms),
          color: kindColor(c.kind),
          hay: `${c.label} ${c.guid} ${c.seat === null ? "" : meta.seats[c.seat].id} ${c.tags.join(" ")}`,
          run: () => {
            s.select(c.guid);
            ui.setPanel("detail");
          },
        }),
      );
    }
    return out;
  }, [snapshot, insights, selected, theme]);
}

function search(commands: Command[], q: string): Command[] {
  const terms = q.toLowerCase().split(/\s+/).filter(Boolean);
  if (!terms.length) return commands.filter((c) => c.group !== "Conversations" && c.group !== "Filters").slice(0, MAX);
  const scored: { c: Command; score: number }[] = [];
  for (const c of commands) {
    let score = 0;
    let ok = true;
    for (const t of terms) {
      const at = c.hay.indexOf(t);
      if (at < 0) {
        ok = false;
        break;
      }
      score += at === 0 || c.hay[at - 1] === " " ? 0 : 1 + at / 100;
    }
    if (!ok) continue;
    // the whole query inside the label beats scattered term hits; a label that starts with it wins
    const label = c.label.toLowerCase();
    const whole = q.trim().toLowerCase();
    if (label === whole) score -= 30;
    else if (label.startsWith(whole)) score -= 20;
    else if (label.includes(whole)) score -= 10;
    scored.push({ c, score: score + (c.group === "Conversations" ? 0.5 : 0) });
  }
  return scored
    .sort((a, b) => a.score - b.score)
    .slice(0, MAX)
    .map((x) => x.c);
}

export function CommandPalette() {
  const open = useUi((s) => s.paletteOpen);
  const setPalette = useUi((s) => s.setPalette);
  const dialog = useRef<HTMLDialogElement>(null);
  const listRef = useRef<HTMLUListElement>(null);
  const [q, setQ] = useState("");
  const [cursor, setCursor] = useState(0);
  const commands = useCommands();
  const results = useMemo(() => search(commands, q), [commands, q]);

  useEffect(() => {
    const d = dialog.current;
    if (!d) return;
    if (open && !d.open) d.showModal();
    else if (!open && d.open) d.close();
    // reset while closed, so the next open never flashes the last query
    if (!open) {
      setQ("");
      setCursor(0);
    }
  }, [open]);

  useEffect(() => setCursor(0), [q]);
  useEffect(() => {
    listRef.current?.querySelector(`[data-i="${cursor}"]`)?.scrollIntoView({ block: "nearest" });
  }, [cursor]);

  const run = (c: Command | undefined) => {
    if (!c) return;
    setPalette(false);
    c.run();
  };

  let lastGroup = "";
  return (
    <dialog ref={dialog} className="sh-palette" aria-label="Search and commands" onClose={() => setPalette(false)} onClick={(e) => e.target === dialog.current && setPalette(false)}>
      <div className="sh-palette__box">
        <input
          className="sh-palette__input"
          role="combobox"
          aria-expanded="true"
          aria-controls="sh-palette-list"
          aria-activedescendant={results[cursor] ? `sh-cmd-${cursor}` : undefined}
          placeholder="Jump to a conversation, view, filter or insight"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setCursor((c) => Math.min(results.length - 1, c + 1));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setCursor((c) => Math.max(0, c - 1));
            } else if (e.key === "Enter") {
              e.preventDefault();
              run(results[cursor]);
            }
          }}
        />
        <ul ref={listRef} id="sh-palette-list" className="sh-palette__list" role="listbox" aria-label="Results">
          {results.map((c, i) => {
            const header = c.group !== lastGroup ? c.group : null;
            lastGroup = c.group;
            return (
              <li key={c.id} role="presentation">
                {header ? <p className="sh-palette__group" aria-hidden="true">{header}</p> : null}
                <div
                  id={`sh-cmd-${i}`}
                  data-i={i}
                  role="option"
                  aria-selected={i === cursor}
                  className="sh-palette__item"
                  onPointerMove={() => setCursor(i)}
                  onClick={() => run(c)}
                >
                  {c.color ? <Swatch color={c.color} /> : <span className="sh-palette__nodot" />}
                  <span className="sh-palette__label">{c.label}</span>
                  {c.hint ? <span className="sh-muted">{c.hint}</span> : null}
                </div>
              </li>
            );
          })}
          {results.length === 0 ? <li className="sh-palette__none">Nothing matches “{q}”. Try a label, a guid prefix or a seat name.</li> : null}
        </ul>
        <footer className="sh-palette__foot">
          <span>
            <Kbd>↑</Kbd>
            <Kbd>↓</Kbd> move
          </span>
          <span>
            <Kbd>↵</Kbd> run
          </span>
          <span>
            <Kbd>esc</Kbd> close
          </span>
        </footer>
      </div>
    </dialog>
  );
}
