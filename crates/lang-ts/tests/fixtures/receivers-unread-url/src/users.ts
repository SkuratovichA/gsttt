import { api } from './http';

export function load(url: string) {
  return api.get(url);
}
