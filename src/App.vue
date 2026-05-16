<script setup lang="ts">
import { ref } from 'vue';
import { usePairsStore } from './stores/pairs';
import EnvCheck from './components/EnvCheck.vue';
import StatusBar from './components/StatusBar.vue';
import PairList from './components/PairList.vue';
import PairForm from './components/PairForm.vue';
import SyncDialog from './components/SyncDialog.vue';
import type { DirectoryPair, SyncDirection } from './types';

const ready = ref(false);
const hostname = ref('');
const store = usePairsStore();

const showForm = ref(false);
const editingPair = ref<DirectoryPair | null>(null);

const showSync = ref(false);
const syncPair = ref<DirectoryPair | null>(null);
const syncDirection = ref<SyncDirection>('push');

function onEnvReady(host: string) {
  hostname.value = host;
  ready.value = true;
  store.refresh();
}

function openAdd() { editingPair.value = null; showForm.value = true; }
function openEdit(p: DirectoryPair) { editingPair.value = p; showForm.value = true; }
async function onSave(pair: DirectoryPair) {
  if (pair.id) await store.update(pair);
  else await store.add(pair);
  showForm.value = false;
}

function openSync(p: DirectoryPair, dir: SyncDirection) {
  syncPair.value = p;
  syncDirection.value = dir;
  showSync.value = true;
}

function onSyncDone() {
  // Refresh to pick up last_sync (the backend should update pair on completion).
  store.refresh();
}
</script>

<template>
  <div class="app">
    <EnvCheck v-if="!ready" @ready="onEnvReady" />
    <template v-else>
      <StatusBar :hostname="hostname" :tailscale-connected="true" />
      <PairList @add="openAdd" @edit="openEdit" @sync="openSync" />
    </template>

    <div v-if="showForm" class="modal-backdrop" @click.self="showForm = false">
      <div class="modal-shell">
        <PairForm :initial="editingPair" @save="onSave" @cancel="showForm = false" />
      </div>
    </div>

    <SyncDialog
      v-if="showSync && syncPair"
      :pair="syncPair"
      :direction="syncDirection"
      @close="showSync = false"
      @done="onSyncDone"
    />
  </div>
</template>

<style>
body { margin: 0; font-family: -apple-system, BlinkMacSystemFont, sans-serif; }
.app { min-height: 100vh; background: white; }
.modal-backdrop { position: fixed; inset: 0; background: rgba(0,0,0,0.4); display: flex; align-items: center; justify-content: center; z-index: 50; }
.modal-shell { background: white; border-radius: 8px; max-height: 90vh; overflow: auto; }
</style>
