<script setup lang="ts">
import { usePairsStore } from '../stores/pairs';
import { formatRelativeTime } from '../lib/format';
import type { DirectoryPair, SyncDirection } from '../types';

const store = usePairsStore();

const emit = defineEmits<{
  add: [];
  edit: [pair: DirectoryPair];
  sync: [pair: DirectoryPair, direction: SyncDirection];
}>();

function lastSyncText(p: DirectoryPair): string {
  if (!p.last_sync) return '尚未同步';
  const dir = p.last_sync.direction === 'push' ? '推过去' : '拉回来';
  const when = formatRelativeTime(p.last_sync.timestamp);
  const status = { success: '已完成', failed: '失败', interrupted: '已中断' }[p.last_sync.status];
  return `${dir} · ${when} ${status}`;
}

function statusClass(p: DirectoryPair): string {
  if (!p.last_sync) return '';
  return p.last_sync.status;
}
</script>

<template>
  <div class="pair-list">
    <div v-if="store.pairs.length === 0" class="empty">
      还没有目录对。点击下方"+ 新建目录对"开始。
    </div>
    <table v-else>
      <thead>
        <tr>
          <th>名称</th>
          <th>路径</th>
          <th>上次同步</th>
          <th></th>
        </tr>
      </thead>
      <tbody>
        <tr v-for="p in store.pairs" :key="p.id">
          <td><strong>{{ p.name }}</strong></td>
          <td class="paths">
            <code>{{ p.local_path }}</code>
            <span class="arrow">↔</span>
            <code>{{ p.remote_host }}:{{ p.remote_path }}</code>
          </td>
          <td :class="['last-sync', statusClass(p)]">{{ lastSyncText(p) }}</td>
          <td class="actions">
            <button class="push" @click="emit('sync', p, 'push')">推过去</button>
            <button class="pull" @click="emit('sync', p, 'pull')">拉回来</button>
            <button class="edit" @click="emit('edit', p)">⋯</button>
          </td>
        </tr>
      </tbody>
    </table>
    <div class="footer">
      <button class="add" @click="emit('add')">+ 新建目录对</button>
    </div>
  </div>
</template>

<style scoped>
.pair-list { padding: 16px; }
.empty { padding: 48px; text-align: center; color: #888; }
table { width: 100%; border-collapse: collapse; }
th, td { padding: 12px 8px; text-align: left; border-bottom: 1px solid #eee; vertical-align: middle; font-size: 13px; }
.paths { font-family: ui-monospace, monospace; color: #555; }
.paths code { background: #f5f5f7; padding: 2px 6px; border-radius: 3px; }
.arrow { margin: 0 6px; color: #888; }
.last-sync.success { color: #2a7; }
.last-sync.failed { color: #c33; }
.last-sync.interrupted { color: #d80; }
.actions { white-space: nowrap; text-align: right; }
.actions button { margin-left: 6px; padding: 5px 12px; }
.actions .push { background: #2563eb; color: white; border: 0; border-radius: 4px; }
.actions .pull { background: #16a34a; color: white; border: 0; border-radius: 4px; }
.actions .edit { background: transparent; border: 0; color: #666; }
.footer { padding: 16px 0; }
.add { padding: 6px 16px; }
</style>
