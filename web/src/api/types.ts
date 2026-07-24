// Mirrors the serde types returned by visuara-signaling's /api/v1 JSON API
// (see visuara-signaling/src/api/*.rs).

export type AccountRole = 'user' | 'admin';

export interface AccountView {
  id: number;
  email: string;
  role: AccountRole;
}

export interface RegistrationStatus {
  enabled: boolean;
}

export interface DeviceSummary {
  device_id: string;
  name: string;
  online: boolean;
  unattended_access_enabled: boolean;
}

export interface SettingsView {
  server_url: string;
  default_device_name: string;
}

export interface ClientBuildStatus {
  platform: string;
  status: string;
}

export interface FetchReleaseResult {
  platform: string;
  version: string | null;
  error: string | null;
}

export interface AccountSummaryView {
  id: number;
  email: string;
  created_at: number;
  role: AccountRole;
}

export interface DeviceView {
  id: string;
  name: string;
  online: boolean;
  unattended_access_enabled: boolean;
}

export interface AccountDetailView {
  id: number;
  email: string;
  role: AccountRole;
  devices: DeviceView[];
}

export interface CreatedAccountView {
  id: number;
  email: string;
  role: AccountRole;
}

export interface RegistrationSetting {
  enabled: boolean;
}

export interface DownloadOption {
  platform: string;
  available: boolean;
  recommended: boolean;
  custom_build: boolean;
  version: string | null;
  fetched_at: number | null;
}

export interface DownloadListView {
  server_url: string | null;
  options: DownloadOption[];
}
