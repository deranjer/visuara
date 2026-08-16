import { api } from './client';
import type { DownloadListView } from './types';

export function listDownloads() {
  return api.get<DownloadListView>('/download');
}

export function downloadUrl(platform: string) {
  return `/api/v1/download/${encodeURIComponent(platform)}`;
}
