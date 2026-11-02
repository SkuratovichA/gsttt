import { BASE } from './config';

export function list(BASE: string) {
  return fetch(`${BASE}/users`);
}

fetch(`${BASE}/orders`);
fetch(BASE);

export function items() {
  return fetch(`${BASE}/items`);
}
