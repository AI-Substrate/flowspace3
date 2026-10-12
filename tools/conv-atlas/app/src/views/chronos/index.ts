import { lazy } from "react";
import type { ViewDef } from "../../core/registry";

// Each view is its own chunk; the shell wraps the stage in <Suspense>.
const Composition = lazy(() => import("./Composition").then((m) => ({ default: m.Composition })));
const Overview = lazy(() => import("./Overview").then((m) => ({ default: m.Overview })));
const River = lazy(() => import("./River").then((m) => ({ default: m.River })));
const Tags = lazy(() => import("./Tags").then((m) => ({ default: m.Tags })));
const Timeline = lazy(() => import("./Timeline").then((m) => ({ default: m.Timeline })));

/** Owned by Chronos: snapshot overview, fleet timeline, session river, composition, tag matrix. */
export const views: ViewDef[] = [
  {
    id: "overview",
    title: "Overview",
    description: "Snapshot landing page: corpus KPIs, growth, top communities and seats, harness/model mix, caveats",
    group: "chronos",
    hotkey: "o",
    component: Overview,
  },
  {
    id: "timeline",
    title: "Timeline",
    description: "Fleet timeline: daily turns stacked by repo, harness, community or kind",
    group: "chronos",
    hotkey: "t",
    component: Timeline,
  },
  {
    id: "river",
    title: "River",
    description: "Session topic river: novelty, labelled topic shifts and a turn map for one long session",
    group: "chronos",
    hotkey: "r",
    component: River,
  },
  {
    id: "composition",
    title: "Composition",
    description: "What conversations are made of: turn class and origin mix, autonomy index, small multiples",
    group: "chronos",
    hotkey: "c",
    component: Composition,
  },
  {
    id: "tags",
    title: "Tags",
    description: "Tag co-occurrence matrix, clustered so tags that travel together form blocks",
    group: "chronos",
    hotkey: "x",
    component: Tags,
  },
];
