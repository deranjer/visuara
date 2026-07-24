import { api } from './client';
import type {
  AccountDetailView,
  AccountRole,
  AccountSummaryView,
  ClientBuildStatus,
  CreatedAccountView,
  FetchReleaseResult,
  RegistrationSetting,
  SettingsView,
} from './types';

export function getSettings() {
  return api.get<SettingsView>('/admin/settings');
}

export function saveSettings(serverUrl: string, defaultDeviceName: string) {
  return api.put<SettingsView>('/admin/settings', { server_url: serverUrl, default_device_name: defaultDeviceName });
}

export function clientBuilds() {
  return api.get<ClientBuildStatus[]>('/admin/client-builds');
}

export function fetchRelease() {
  return api.post<FetchReleaseResult[]>('/admin/client-builds/fetch');
}

export function listAccounts() {
  return api.get<AccountSummaryView[]>('/admin/accounts');
}

export function accountDetail(id: number) {
  return api.get<AccountDetailView>(`/admin/accounts/${id}`);
}

export function createAccount(email: string, password: string, role?: AccountRole) {
  return api.post<CreatedAccountView>('/admin/accounts', { email, password, role });
}

export function setAccountRole(id: number, role: AccountRole) {
  return api.put<void>(`/admin/accounts/${id}/role`, { role });
}

export function deleteAccount(id: number) {
  return api.delete<void>(`/admin/accounts/${id}`);
}

export function deleteAccountDevice(accountId: number, deviceId: string) {
  return api.delete<void>(`/admin/accounts/${accountId}/devices/${encodeURIComponent(deviceId)}`);
}

export function getRegistration() {
  return api.get<RegistrationSetting>('/admin/registration');
}

export function setRegistration(enabled: boolean) {
  return api.put<RegistrationSetting>('/admin/registration', { enabled });
}
