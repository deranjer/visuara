import { api } from './client';
import type { AccountView, RegistrationStatus } from './types';

export function register(email: string, password: string) {
  return api.post<AccountView>('/auth/register', { email, password });
}

export function login(email: string, password: string) {
  return api.post<AccountView>('/auth/login', { email, password });
}

export function logout() {
  return api.post<void>('/auth/logout');
}

export function me() {
  return api.get<AccountView>('/auth/me');
}

export function registrationStatus() {
  return api.get<RegistrationStatus>('/registration-status');
}
