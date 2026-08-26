import { create } from "zustand";
import type { ProgressState, ResourceItem, ZebraProduct } from "../lib/schema";

type ProductCollectorState = {
  resources: ResourceItem[];
  selected: string[];
  progress: Record<string, ProgressState>;
};

type ProgressUpdate = {
  product: ZebraProduct;
  id: string;
  value: ProgressState;
};

type CollectorState = {
  outputDir: string;
  products: Record<ZebraProduct, ProductCollectorState>;
  setOutputDir: (path: string) => void;
  setResources: (product: ZebraProduct, items: ResourceItem[]) => void;
  toggle: (product: ZebraProduct, id: string) => void;
  selectAll: (product: ZebraProduct, ids: string[]) => void;
  clearSelection: (product: ZebraProduct) => void;
  updateProgress: (
    product: ZebraProduct,
    id: string,
    value: ProgressState,
  ) => void;
  updateProgressBatch: (updates: ProgressUpdate[]) => void;
};

const emptyProductState = (): ProductCollectorState => ({
  resources: [],
  selected: [],
  progress: {},
});

export const useCollectorStore = create<CollectorState>((set) => ({
  outputDir: "G:\\workspace\\banma\\downloads",
  products: {
    pedia: emptyProductState(),
    aioral: emptyProductState(),
    zebra: emptyProductState(),
  },
  setOutputDir: (outputDir) => set({ outputDir }),
  setResources: (product, resources) =>
    set((state) => ({
      products: {
        ...state.products,
        [product]: {
          ...state.products[product],
          resources,
          selected: resources.map((item) => item.id),
        },
      },
    })),
  toggle: (product, id) =>
    set((state) => {
      const current = state.products[product];
      return {
        products: {
          ...state.products,
          [product]: {
            ...current,
            selected: current.selected.includes(id)
              ? current.selected.filter((value) => value !== id)
              : [...current.selected, id],
          },
        },
      };
    }),
  selectAll: (product, selected) =>
    set((state) => ({
      products: {
        ...state.products,
        [product]: { ...state.products[product], selected },
      },
    })),
  clearSelection: (product) =>
    set((state) => ({
      products: {
        ...state.products,
        [product]: { ...state.products[product], selected: [] },
      },
    })),
  updateProgress: (product, id, value) =>
    set((state) => {
      const current = state.products[product];
      return {
        products: {
          ...state.products,
          [product]: {
            ...current,
            progress: { ...current.progress, [id]: value },
          },
        },
      };
    }),
  updateProgressBatch: (updates) =>
    set((state) => {
      const nextProgress: Partial<
        Record<ZebraProduct, Record<string, ProgressState>>
      > = {};
      for (const { product, id, value } of updates) {
        const progress = nextProgress[product] ?? {
          ...state.products[product].progress,
        };
        progress[id] = value;
        nextProgress[product] = progress;
      }

      const products = { ...state.products };
      for (const product of Object.keys(nextProgress) as ZebraProduct[]) {
        products[product] = {
          ...products[product],
          progress: nextProgress[product]!,
        };
      }
      return { products };
    }),
}));

export function useCollector(product: ZebraProduct) {
  const productState = useCollectorStore((state) => state.products[product]);
  const outputDir = useCollectorStore((state) => state.outputDir);
  const setOutputDir = useCollectorStore((state) => state.setOutputDir);
  const setResources = useCollectorStore((state) => state.setResources);
  const toggle = useCollectorStore((state) => state.toggle);
  const selectAll = useCollectorStore((state) => state.selectAll);
  const clearSelection = useCollectorStore((state) => state.clearSelection);
  const updateProgress = useCollectorStore((state) => state.updateProgress);

  return {
    ...productState,
    outputDir,
    setOutputDir,
    setResources: (items: ResourceItem[]) => setResources(product, items),
    toggle: (id: string) => toggle(product, id),
    selectAll: (ids: string[]) => selectAll(product, ids),
    clearSelection: () => clearSelection(product),
    updateProgress: (id: string, value: ProgressState) =>
      updateProgress(product, id, value),
  };
}
