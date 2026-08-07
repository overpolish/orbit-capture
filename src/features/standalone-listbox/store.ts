import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

export type StandaloneListboxItem = {
  id: string;
  label: string;
  iconPath?: string | null;
};

type OpenListbox = {
  id: string;
  items: StandaloneListboxItem[];
  label: string;
  selectedIds: string[];
  selectionMode: "multiple" | "single";
  exclusiveId?: string;
};

type ListboxSelection = {
  eventId: string;
  id: string;
  selectedIds: string[];
};

type StandaloneListboxStore = {
  active: OpenListbox | null;
  close: () => void;
  lastSelection: ListboxSelection | null;
  open: (listbox: OpenListbox) => void;
  select: (id: string, selectedIds: string[]) => void;
};

const STORE_NAME = "orbit-capture-standalone-listbox";
const SELECTION_STORE_NAME = `${STORE_NAME}-selection`;

export const useStandaloneListboxStore = create<StandaloneListboxStore>()(
  persist(
    (set) => ({
      active: null,
      close: () => {
        set({ active: null });
      },
      lastSelection: null,
      open: (active) => {
        set({ active });
      },
      select: (id, selectedIds) => {
        const lastSelection = {
          eventId: crypto.randomUUID(),
          id,
          selectedIds,
        };
        set((state) => ({
          active:
            state.active?.id === id
              ? { ...state.active, selectedIds }
              : state.active,
          lastSelection,
        }));
        localStorage.setItem(
          SELECTION_STORE_NAME,
          JSON.stringify(lastSelection),
        );
      },
    }),
    {
      name: STORE_NAME,
      partialize: (state) => ({ active: state.active }),
      storage: createJSONStorage(() => localStorage),
    },
  ),
);

export const synchronizeStandaloneListboxStore = (event: StorageEvent) => {
  if (event.key === STORE_NAME) {
    void useStandaloneListboxStore.persist.rehydrate();
  } else if (event.key === SELECTION_STORE_NAME && event.newValue) {
    try {
      const lastSelection = JSON.parse(event.newValue) as ListboxSelection;
      useStandaloneListboxStore.setState({ lastSelection });
    } catch {
      // Ignore malformed cross-window messages.
    }
  }
};
