import { invoke as rawInvoke } from '@tauri-apps/api/core';
import { listen as rawListen, type UnlistenFn } from '@tauri-apps/api/event';
import type {
  DirectoryPair, TailnetDevice, EnvCheckResult,
  DryRunSummary, ProgressUpdate, SyncResult, PathProbeResult,
  SyncDirection,
} from '../types';

export const api = {
  listPairs: () => rawInvoke<DirectoryPair[]>('list_pairs'),
  addPair: (pair: DirectoryPair) => rawInvoke<DirectoryPair>('add_pair', { pair }),
  updatePair: (pair: DirectoryPair) => rawInvoke<DirectoryPair>('update_pair', { pair }),
  deletePair: (id: string) => rawInvoke<void>('delete_pair', { id }),

  listTailnetDevices: () => rawInvoke<TailnetDevice[]>('list_tailnet_devices'),
  envCheck: () => rawInvoke<EnvCheckResult>('env_check'),

  probeRemotePath: (user: string, host: string, path: string) =>
    rawInvoke<PathProbeResult>('probe_remote_path', { user, host, path }),
  createRemoteDir: (user: string, host: string, path: string) =>
    rawInvoke<void>('create_remote_dir', { user, host, path }),

  dryRun: (pairId: string, direction: SyncDirection) =>
    rawInvoke<DryRunSummary>('dry_run_sync', { req: { pair_id: pairId, direction } }),
  startSync: (pairId: string, direction: SyncDirection) =>
    rawInvoke<string>('start_sync', { req: { pair_id: pairId, direction } }),
  cancelSync: (taskId: string) =>
    rawInvoke<boolean>('cancel_sync', { taskId }),

  openFullDiskAccess: () => rawInvoke<void>('open_full_disk_access'),
};

export async function onSyncProgress(taskId: string, cb: (p: ProgressUpdate) => void): Promise<UnlistenFn> {
  return rawListen<ProgressUpdate>(`sync-progress:${taskId}`, e => cb(e.payload));
}

export async function onSyncDone(taskId: string, cb: (r: SyncResult) => void): Promise<UnlistenFn> {
  return rawListen<SyncResult>(`sync-done:${taskId}`, e => cb(e.payload));
}
