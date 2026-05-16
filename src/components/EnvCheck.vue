<script setup lang="ts">
import { ref, onMounted } from 'vue';
import { api } from '../lib/tauri';
import type { EnvCheckResult } from '../types';

const result = ref<EnvCheckResult | null>(null);
const checking = ref(false);

async function check() {
  checking.value = true;
  try {
    result.value = await api.envCheck();
  } finally {
    checking.value = false;
  }
}

onMounted(check);

const emit = defineEmits<{ ready: [hostname: string] }>();

function proceed() {
  if (result.value?.tailscale_installed && result.value.tailscale_logged_in && result.value.tailscale_ssh_enabled) {
    emit('ready', result.value.self_hostname || '');
  }
}

// Auto-emit when all green
import { watch } from 'vue';
watch(result, () => proceed(), { immediate: false });
</script>

<template>
  <div class="env-check">
    <h2>环境检查</h2>
    <div v-if="checking">检测中...</div>
    <div v-else-if="result">
      <ul>
        <li :class="{ ok: result.tailscale_installed }">
          Tailscale CLI 已安装：{{ result.tailscale_installed ? '是' : '否' }}
        </li>
        <li v-if="result.tailscale_installed" :class="{ ok: result.tailscale_logged_in }">
          已登录 tailnet：{{ result.tailscale_logged_in ? '是' : '否' }}
        </li>
        <li v-if="result.tailscale_logged_in" :class="{ ok: result.tailscale_ssh_enabled }">
          Tailscale SSH 已启用：{{ result.tailscale_ssh_enabled ? '是' : '否' }}
        </li>
      </ul>

      <div v-if="!result.tailscale_installed" class="hint">
        请先安装 Tailscale：<a href="https://tailscale.com/download" target="_blank">tailscale.com/download</a>
      </div>
      <div v-else-if="!result.tailscale_logged_in" class="hint">
        请在菜单栏 Tailscale 图标登录你的 tailnet。
      </div>
      <div v-else-if="!result.tailscale_ssh_enabled" class="hint">
        请在终端执行：<code>tailscale set --ssh</code>
      </div>

      <button @click="check">重新检测</button>
    </div>
  </div>
</template>

<style scoped>
.env-check { padding: 24px; max-width: 480px; margin: 0 auto; }
ul { list-style: none; padding: 0; }
li { padding: 6px 0; color: #c33; }
li.ok { color: #2a7; }
li.ok::before { content: '✓ '; }
li:not(.ok)::before { content: '✗ '; }
.hint { margin-top: 12px; padding: 12px; background: #fff8e1; border-radius: 4px; }
button { margin-top: 16px; padding: 6px 16px; }
</style>
