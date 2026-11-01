import axios from 'axios';

function before() {
  const api = load();
  return api;
}

export const api = axios.create({ baseURL: '/a' });

export function after() {
  const api = load();
  return api;
}
