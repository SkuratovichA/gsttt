import axios from 'axios';

const cache = { request: (config: { url: string }) => config.url };
export const hit = cache.request({ url: '/x' });
export const client = axios;
