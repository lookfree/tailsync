<script setup lang="ts">
import { ref, watch, onMounted } from 'vue';
import { open } from '@tauri-apps/plugin-dialog';
import { api } from '../lib/tauri';
import type { DirectoryPair, TailnetDevice, PathProbeResult } from '../types';

const props = defineProps<{ initial?: DirectoryPair | null }>();
const emit = defineEmits<{ save: [pair: DirectoryPair]; cancel: [] }>();

function blank(): DirectoryPair {
  return {
    id: '',
    name: '',
    local_path: '',
    remote_host: '',
    remote_user: '',
    remote_path: '',
    excludes: [],
    bandwidth_limit_kbps: null,
    mirror_mode: false,
    last_sync: null,
  };
}

const form = ref<DirectoryPair>({ ...(props.initial ?? blank()) });
const excludesText = ref(form.value.excludes.join('\n'));
const bwlimitText = ref(form.value.bandwidth_limit_kbps?.toString() ?? '');

const tailnetDevices = ref<TailnetDevice[]>([]);
const remoteProbe = ref<PathProbeResult | null>(null);

onMounted(async () => {
  try {
    tailnetDevices.value = (await api.listTailnetDevices()).filter(d => !d.is_self);
  } catch (_) { /* noop */ }
});

async function pickLocalPath() {
  const selected = await open({ directory: true, multiple: false });
  if (typeof selected === 'string') form.value.local_path = selected;
}

async function probePath() {
  if (!form.value.remote_user || !form.value.remote_host || !form.value.remote_path) {
    remoteProbe.value = null; return;
  }
  remoteProbe.value = await api.probeRemotePath(
    form.value.remote_user, form.value.remote_host, form.value.remote_path
  );
}

watch([
  () => form.value.remote_user,
  () => form.value.remote_host,
  () => form.value.remote_path,
], () => probePath());

function save() {
  form.value.excludes = excludesText.value.split('\n').map(s => s.trim()).filter(Boolean);
  const n = parseInt(bwlimitText.value, 10);
  form.value.bandwidth_limit_kbps = isNaN(n) || n <= 0 ? null : n;
  emit('save', { ...form.value });
}

function probeIcon(): string {
  if (!remoteProbe.value) return '';
  if (remoteProbe.value === 'Exists') return '✓ 路径存在';
  if (remoteProbe.value === 'Missing') return '✗ 路径不存在（可保存，首次同步时再创建）';
  return `⚠ SSH 失败：${(remoteProbe.value as { SshFailed: string }).SshFailed}`;
}

function probeClass(): string {
  if (!remoteProbe.value) return '';
  if (remoteProbe.value === 'Exists') return 'ok';
  return 'warn';
}
</script>

<template>
  <div class="pair-form">
    <h3>{{ initial ? '编辑目录对' : '新建目录对' }}</h3>

    <label>备注名
      <input v-model="form.name" placeholder="如：读书笔记" />
    </label>

    <label>本机路径
      <div class="row">
        <input v-model="form.local_path" placeholder="/Users/me/Documents/notes" />
        <button @click="pickLocalPath">选择…</button>
      </div>
    </label>

    <label>对端机器
      <select v-model="form.remote_host">
        <option value="" disabled>选择 tailnet 中的机器</option>
        <option v-for="d in tailnetDevices" :key="d.hostname" :value="d.hostname"
                :disabled="!d.online">
          {{ d.hostname }} {{ d.online ? '' : '(离线)' }}
        </option>
      </select>
    </label>

    <label>对端登录用户
      <input v-model="form.remote_user" placeholder="对端的 macOS 账号名" />
    </label>

    <label>对端路径
      <input v-model="form.remote_path" placeholder="/Users/peer/sync/notes" />
      <span :class="['probe', probeClass()]">{{ probeIcon() }}</span>
    </label>

    <label>排除规则（每行一个 glob，如 .DS_Store / node_modules/ / *.tmp）
      <textarea v-model="excludesText" rows="4"></textarea>
    </label>

    <label>限速（KB/s，0 或留空表示不限速）
      <input v-model="bwlimitText" type="number" min="0" />
    </label>

    <label class="checkbox">
      <input type="checkbox" v-model="form.mirror_mode" />
      镜像模式（开启后会从对端删除本机已删除的文件，危险操作）
    </label>

    <div class="actions">
      <button @click="emit('cancel')">取消</button>
      <button class="primary" @click="save">保存</button>
    </div>
  </div>
</template>

<style scoped>
.pair-form { padding: 24px; max-width: 560px; }
label { display: block; margin-bottom: 14px; font-size: 13px; color: #444; }
label > input, label > select, label > textarea {
  display: block; width: 100%; margin-top: 4px; padding: 6px 8px; box-sizing: border-box;
}
.row { display: flex; gap: 6px; margin-top: 4px; }
.row input { flex: 1; }
label.checkbox { display: flex; align-items: center; gap: 6px; }
label.checkbox > input { width: auto; margin: 0; }
.probe { display: block; margin-top: 4px; font-size: 12px; }
.probe.ok { color: #2a7; }
.probe.warn { color: #d80; }
.actions { display: flex; gap: 8px; justify-content: flex-end; margin-top: 16px; }
.actions .primary { background: #2563eb; color: white; border: 0; padding: 6px 16px; border-radius: 4px; }
</style>
