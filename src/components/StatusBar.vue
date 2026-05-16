<script setup lang="ts">
defineProps<{ hostname: string; tailscaleConnected: boolean }>();
</script>

<template>
  <div class="status-bar">
    <div class="left">
      <span class="label">本机：</span>
      <span class="value">{{ hostname || '未知' }}</span>
    </div>
    <div class="right">
      <span class="label">Tailscale：</span>
      <span :class="['dot', tailscaleConnected ? 'ok' : 'bad']"></span>
      <span>{{ tailscaleConnected ? '已连接' : '未连接' }}</span>
    </div>
  </div>
</template>

<style scoped>
.status-bar {
  display: flex;
  justify-content: space-between;
  padding: 8px 16px;
  background: #f5f5f7;
  border-bottom: 1px solid #e0e0e2;
  font-size: 13px;
}
.label { color: #666; }
.value { font-weight: 500; }
.dot { display: inline-block; width: 8px; height: 8px; border-radius: 50%; margin: 0 6px; }
.dot.ok { background: #2a7; }
.dot.bad { background: #c33; }
</style>
