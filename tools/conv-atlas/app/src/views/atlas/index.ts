import { lazy } from "react";
import type { ViewDef } from "../../core/registry";

// each view (with regl-scatterplot or sigma) loads on first visit; the shell provides Suspense
const Galaxy = lazy(() => import("./galaxy/Galaxy").then((m) => ({ default: m.Galaxy })));
const SimilarityGraph = lazy(() => import("./graph/SimilarityGraph").then((m) => ({ default: m.SimilarityGraph })));
const Seats = lazy(() => import("./seats/Seats").then((m) => ({ default: m.Seats })));

/** Owned by Atlas: galaxy, similarity graph, seat network. */
export const views: ViewDef[] = [
  {
    id: "galaxy",
    title: "Galaxy",
    description: "Conversation map: semantic layout, grows and fades with playback",
    group: "atlas",
    hotkey: "g",
    component: Galaxy,
  },
  {
    id: "graph",
    title: "Graph",
    description: "Similarity graph: kNN links laid out by force, cross-repo bridges highlighted",
    group: "atlas",
    hotkey: "k",
    component: SimilarityGraph,
  },
  {
    id: "seats",
    title: "Seats",
    description: "Seat network: who messages whom over pij up to the playhead; today's messages fire",
    group: "atlas",
    hotkey: "s",
    component: Seats,
  },
];
