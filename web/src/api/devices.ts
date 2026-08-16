import { api } from './client';
import type { DeviceSummary } from './types';

export function listDevices() {
  return api.get<DeviceSummary[]>('/devices');
}

export function deleteDevice(deviceId: string) {
  return api.delete<void>(`/devices/${encodeURIComponent(deviceId)}`);
}
