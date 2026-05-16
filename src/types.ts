export type SyncDirection = 'push' | 'pull';
export type SyncStatus = 'success' | 'failed' | 'interrupted';

export interface LastSync {
  direction: SyncDirection;
  timestamp: number;
  status: SyncStatus;
  message: string;
}

export interface DirectoryPair {
  id: string;
  name: string;
  local_path: string;
  remote_host: string;
  remote_user: string;
  remote_path: string;
  excludes: string[];
  bandwidth_limit_kbps: number | null;
  mirror_mode: boolean;
  last_sync: LastSync | null;
}

export interface TailnetDevice {
  hostname: string;
  tailscale_ip: string;
  user: string;
  os: string;
  online: boolean;
  is_self: boolean;
  ssh_enabled: boolean;
}

export interface EnvCheckResult {
  tailscale_installed: boolean;
  tailscale_logged_in: boolean;
  tailscale_ssh_enabled: boolean;
  rsync_modern: boolean;
  self_hostname: string | null;
  error_detail: string | null;
}

export interface DryRunSummary {
  files_to_copy: number;
  files_to_delete: number;
  files_to_update: number;
  total_bytes: number;
  file_list: string[];
}

export interface ProgressUpdate {
  bytes_transferred: number;
  total_bytes: number | null;
  percent: number | null;
  rate_bps: number | null;
  eta_seconds: number | null;
  current_file: string | null;
}

export interface SyncResult {
  exit_code: number;
  message: string;
  stderr_tail: string;
}

export type PathProbeResult =
  | 'Exists'
  | 'Missing'
  | { SshFailed: string };
