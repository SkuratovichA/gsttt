import axios from 'axios';

declare const base: string;
declare const host: string;
declare const path: string;

const api = axios.create({ timeout: 1000 });
export const dynamic = () => api.get(`${base}${path}`);
export const users = () => api.get('users');

const edge = axios.create({ baseURL: `${host}` });
export const reach = () => edge.get(`${path}`);

const v1 = axios.create({ baseURL: 'https://api.example.com/v1' });
export const bookings = () => v1.get('/bookings');
