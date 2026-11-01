import axios from 'axios';

export const api = axios.create({ baseURL: '/a' });

export function other() {
  const api = axios.create({ baseURL: '/b' });
  return api;
}
