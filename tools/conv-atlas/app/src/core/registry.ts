/**
 * The view registry. View owners export `views: ViewDef[]` from their own
 * `src/views/<group>/index.ts`; the shell renders navigation and the active view
 * from this list. Adding a view never touches core or shell code.
 */
import type { ComponentType } from "react";
import { views as atlasViews } from "../views/atlas";
import { views as chronosViews } from "../views/chronos";

export interface ViewDef {
  /** stable id, also the URL hash: #/<id> */
  id: string;
  title: string;
  /** one line for the command palette and tooltips */
  description: string;
  group: "atlas" | "chronos";
  /** single key, used with the shell's view-switch modifier */
  hotkey?: string;
  /** fills the shell's main stage; reads everything from the store */
  component: ComponentType;
}

export const VIEWS: ViewDef[] = [...atlasViews, ...chronosViews];
