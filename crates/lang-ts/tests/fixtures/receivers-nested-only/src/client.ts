import axios from 'axios';

export const api = { get: (url: string) => url };

export function make() {
  const api = axios.create({ baseURL: '/b' });
  return api;
}
