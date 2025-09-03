import { create } from 'zustand';
import { devtools, persist } from 'zustand/middleware';

type ExperimentalStore = {};

export const useExperimental = create<ExperimentalStore>()(
  devtools(
    persist((_set, _get) => ({}), {
      name: 'experimental',
    }),
  ),
);
