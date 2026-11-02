import { BASE } from './config';

export function list(BASE: string) {
  return fetch(`${BASE}/users`);
}

fetch(`${BASE}/orders`);
fetch(BASE);
