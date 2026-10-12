/**
 * Shell-local UI state (which panel, which overlay). Shared app state lives in
 * core/store.ts; nothing here survives a reload except via the URL hash.
 */
import { create } from "zustand";

export type Panel = "detail" | "reader" | "insights";

export interface Toast {
  id: number;
  text: string;
}

interface UiState {
  /** right panel; null = closed */
  panel: Panel | null;
  paletteOpen: boolean;
  keysOpen: boolean;
  /** G was pressed: the next key picks a view */
  go: boolean;
  /** tour mode: index into the insight list, null = not touring */
  tour: number | null;
  toasts: Toast[];
  setPanel(p: Panel | null): void;
  setPalette(open: boolean): void;
  setKeys(open: boolean): void;
  setGo(go: boolean): void;
  setTour(i: number | null): void;
  toast(text: string): void;
}

let toastId = 0;

export const useUi = create<UiState>()((set, get) => ({
  panel: null,
  paletteOpen: false,
  keysOpen: false,
  go: false,
  tour: null,
  toasts: [],
  setPanel: (panel) => set({ panel }),
  setPalette: (paletteOpen) => set({ paletteOpen }),
  setKeys: (keysOpen) => set({ keysOpen }),
  setGo: (go) => set({ go }),
  setTour: (tour) => set({ tour }),
  toast: (text) => {
    const id = ++toastId;
    set({ toasts: [...get().toasts, { id, text }] });
    setTimeout(() => set({ toasts: get().toasts.filter((t) => t.id !== id) }), 4200);
  },
}));
