import { defineStore } from 'pinia';
import { ref } from 'vue';
import { api } from '../lib/tauri';
import type { DirectoryPair } from '../types';

export const usePairsStore = defineStore('pairs', () => {
  const pairs = ref<DirectoryPair[]>([]);
  const loading = ref(false);

  async function refresh() {
    loading.value = true;
    try {
      pairs.value = await api.listPairs();
    } finally {
      loading.value = false;
    }
  }

  async function add(pair: DirectoryPair) {
    const created = await api.addPair(pair);
    pairs.value.push(created);
    return created;
  }

  async function update(pair: DirectoryPair) {
    const updated = await api.updatePair(pair);
    const idx = pairs.value.findIndex(p => p.id === pair.id);
    if (idx >= 0) pairs.value[idx] = updated;
    return updated;
  }

  async function remove(id: string) {
    await api.deletePair(id);
    pairs.value = pairs.value.filter(p => p.id !== id);
  }

  return { pairs, loading, refresh, add, update, remove };
});
