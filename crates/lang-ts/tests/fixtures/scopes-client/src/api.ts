import axios from 'axios';

const BASE = '/api';

export function users() {
  const base = `${BASE}/users`;
  const api = axios.create({ baseURL: base });
  return api.get('/list');
}

export function make(BASE: string) {
  const client = axios.create({ baseURL: BASE });
  return client.get('/orders');
}
