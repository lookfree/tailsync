<script setup lang="ts">
import { ref, onUnmounted } from 'vue';
import { api, onSyncProgress, onSyncDone } from '../lib/tauri';
import { formatBytes, formatRate, formatEta } from '../lib/format';
import type { DirectoryPair, SyncDirection, DryRunSummary, ProgressUpdate, SyncResult } from '../types';

const props = defineProps<{ pair: DirectoryPair; direction: SyncDirection }>();
const emit = defineEmits<{ close: []; done: [result: SyncResult] }>();

type Phase = 'preview-loading' | 'preview-ready' | 'preview-failed' | 'syncing' | 'done' | 'failed';
const phase = ref<Phase>('preview-loading');
const summary = ref<DryRunSummary | null>(null);
const previewError = ref('');
const progress = ref<ProgressUpdate | null>(null);
const result = ref<SyncResult | null>(null);
const taskId = ref('');
let unlistenProgress: (() => void) | null = null;
let unlistenDone: (() => void) | null = null;

const dirText = props.direction === 'push'
  ? `${props.pair.local_path} → ${props.pair.remote_host}:${props.pair.remote_path}`
  : `${props.pair.remote_host}:${props.pair.remote_path} → ${props.pair.local_path}`;

async function loadPreview() {
  try {
    summary.value = await api.dryRun(props.pair.id, props.direction);
    phase.value = 'preview-ready';
  } catch (e: any) {
    previewError.value = typeof e === 'string' ? e : (e?.message || JSON.stringify(e));
    phase.value = 'preview-failed';
  }
}

async function confirm() {
  phase.value = 'syncing';
  try {
    taskId.value = await api.startSync(props.pair.id, props.direction);
    unlistenProgress = await onSyncProgress(taskId.value, p => { progress.value = p; });
    unlistenDone = await onSyncDone(taskId.value, r => {
      result.value = r;
      phase.value = r.exit_code === 0 ? 'done' : 'failed';
      emit('done', r);
    });
  } catch (e: any) {
    previewError.value = typeof e === 'string' ? e : JSON.stringify(e);
    phase.value = 'failed';
  }
}

async function cancel() {
  if (taskId.value) await api.cancelSync(taskId.value);
}

loadPreview();

onUnmounted(() => {
  unlistenProgress?.();
  unlistenDone?.();
});
</script>

<template>
  <div class="modal-backdrop">
    <div class="modal">
      <h3>{{ direction === 'push' ? '推送' : '拉取' }}：{{ pair.name }}</h3>
      <div class="path-line">{{ dirText }}</div>

      <div v-if="phase === 'preview-loading'">分析中…</div>

      <div v-else-if="phase === 'preview-ready' && summary">
        <div class="summary">
          <div>复制 <strong>{{ summary.files_to_copy }}</strong> 个文件</div>
          <div>修改 <strong>{{ summary.files_to_update }}</strong> 个文件</div>
          <div>删除 <strong>{{ summary.files_to_delete }}</strong> 个文件</div>
          <div>共 <strong>{{ formatBytes(summary.total_bytes) }}</strong></div>
        </div>
        <details v-if="summary.file_list.length">
          <summary>查看完整文件列表 ({{ summary.file_list.length }})</summary>
          <pre>{{ summary.file_list.join('\n') }}</pre>
        </details>
        <div class="actions">
          <button @click="emit('close')">取消</button>
          <button class="primary" @click="confirm">确认执行</button>
        </div>
      </div>

      <div v-else-if="phase === 'preview-failed'">
        <div class="error">预览失败</div>
        <details><summary>详细信息</summary><pre>{{ previewError }}</pre></details>
        <div class="actions"><button @click="emit('close')">关闭</button></div>
      </div>

      <div v-else-if="phase === 'syncing'">
        <div class="current-file">{{ progress?.current_file || '准备中…' }}</div>
        <div class="progress-bar">
          <div class="fill" :style="{ width: `${progress?.percent ?? 0}%` }"></div>
        </div>
        <div class="meta">
          <span>{{ progress?.percent?.toFixed(0) ?? 0 }}%</span>
          <span>{{ formatBytes(progress?.bytes_transferred ?? 0) }}</span>
          <span>{{ formatRate(progress?.rate_bps ?? 0) }}</span>
          <span>剩余 {{ formatEta(progress?.eta_seconds ?? null) }}</span>
        </div>
        <div class="actions"><button @click="cancel">取消</button></div>
      </div>

      <div v-else-if="phase === 'done'">
        <div class="ok">同步完成</div>
        <div class="actions"><button class="primary" @click="emit('close')">关闭</button></div>
      </div>

      <div v-else-if="phase === 'failed'">
        <div class="error">{{ result?.message || '同步失败' }}</div>
        <details><summary>错误详情</summary><pre>{{ result?.stderr_tail || previewError }}</pre></details>
        <div class="actions">
          <button @click="emit('close')">关闭</button>
          <button class="primary" @click="loadPreview">重试</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.modal-backdrop {
  position: fixed; inset: 0; background: rgba(0,0,0,0.4);
  display: flex; align-items: center; justify-content: center; z-index: 100;
}
.modal { background: white; border-radius: 8px; padding: 24px; min-width: 480px; max-width: 720px; max-height: 80vh; overflow: auto; }
.path-line { font-family: ui-monospace, monospace; color: #666; font-size: 12px; padding: 8px 0 16px; word-break: break-all; }
.summary { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; padding: 12px; background: #f5f5f7; border-radius: 4px; margin-bottom: 12px; }
.actions { display: flex; gap: 8px; justify-content: flex-end; margin-top: 16px; }
.actions .primary { background: #2563eb; color: white; border: 0; padding: 6px 16px; border-radius: 4px; }
.current-file { font-family: ui-monospace, monospace; font-size: 12px; color: #555; padding: 8px 0; word-break: break-all; }
.progress-bar { height: 8px; background: #eee; border-radius: 4px; overflow: hidden; }
.fill { height: 100%; background: #2563eb; transition: width 0.3s; }
.meta { display: flex; justify-content: space-between; padding-top: 6px; font-size: 12px; color: #666; }
.error { color: #c33; padding: 12px 0; }
.ok { color: #2a7; padding: 12px 0; }
pre { background: #f5f5f7; padding: 12px; border-radius: 4px; max-height: 240px; overflow: auto; font-size: 11px; }
</style>
