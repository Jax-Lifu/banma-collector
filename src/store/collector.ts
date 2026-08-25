import { create } from "zustand";
import type { ProgressState, ResourceItem } from "../lib/schema";

type CollectorState = {
  outputDir: string;
  resources: ResourceItem[];
  selected: string[];
  progress: Record<string, ProgressState>;
  setOutputDir: (path: string) => void;
  setResources: (items: ResourceItem[]) => void;
  toggle: (id: string) => void;
  selectAll: (ids: string[]) => void;
  clearSelection: () => void;
  updateProgress: (id: string, value: ProgressState) => void;
};

export const useCollector = create<CollectorState>((set) => ({
  outputDir: "G:\\workspace\\banma\\downloads",
  resources: [],
  selected: [],
  progress: {},
  setOutputDir: (outputDir) => set({ outputDir }),
  setResources: (resources) =>
    set({
      resources,
      selected: resources.map((item) => item.id),
      progress: {},
    }),
  toggle: (id) =>
    set((s) => ({
      selected: s.selected.includes(id)
        ? s.selected.filter((v) => v !== id)
        : [...s.selected, id],
    })),
  selectAll: (ids) => set({ selected: ids }),
  clearSelection: () => set({ selected: [] }),
  updateProgress: (id, value) =>
    set((s) => ({ progress: { ...s.progress, [id]: value } })),
}));
