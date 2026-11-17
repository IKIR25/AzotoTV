// START › Services: the optional services' settings (services.rs).
import { invoke } from '@tauri-apps/api/core';

export interface ServiceField {
  key: string;
  label: string;
  hint: string;
  kind: 'url' | 'text' | 'secret' | 'dir';
  optional: boolean;
  value: string;
}

export interface Service {
  id: string;
  title: string;
  what: string;
  configured: boolean;
  fields: ServiceField[];
}

export const services = $state<{ list: Service[] }>({ list: [] });

export async function loadServices() {
  services.list = await invoke<Service[]>('services_list');
  return services.list;
}

export const testService = (id: string, values: Record<string, string>) => invoke<string[]>('services_test', { id, values });
export const saveService = (id: string, values: Record<string, string>) => invoke<string[]>('services_save', { id, values });
export const removeService = (id: string) => invoke<void>('services_remove', { id });
export const restartApp = () => invoke<void>('app_restart');
