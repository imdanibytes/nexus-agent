import { create } from "zustand";
import {
  fetchProviders,
  createProvider,
  updateProvider,
  deleteProvider,
  testProvider,
  type ProviderPublic,
  type CreateProviderRequest,
} from "../api/client";

interface ProviderState {
  providers: ProviderPublic[];
  isLoading: boolean;

  loadProviders: () => Promise<void>;
  createProvider: (data: CreateProviderRequest) => Promise<ProviderPublic>;
  updateProvider: (
    id: string,
    data: Partial<CreateProviderRequest>,
  ) => Promise<ProviderPublic>;
  deleteProvider: (id: string) => Promise<void>;
  testProvider: (id: string) => Promise<{ ok: boolean; error?: string }>;
}

export const useProviderStore = create<ProviderState>((set) => ({
  providers: [],
  isLoading: false,

  loadProviders: async () => {
    set({ isLoading: true });
    try {
      const providers = await fetchProviders();
      set({ providers, isLoading: false });
    } catch (err) {
      console.error("Failed to load providers:", err);
      set({ isLoading: false });
    }
  },

  createProvider: async (data) => {
    const provider = await createProvider(data);
    set((s) => ({ providers: [...s.providers, provider] }));
    return provider;
  },

  updateProvider: async (id, data) => {
    const provider = await updateProvider(id, data);
    set((s) => ({
      providers: s.providers.map((p) => (p.id === id ? provider : p)),
    }));
    return provider;
  },

  deleteProvider: async (id) => {
    await deleteProvider(id);
    set((s) => ({ providers: s.providers.filter((p) => p.id !== id) }));
  },

  testProvider: async (id) => {
    return testProvider(id);
  },
}));
