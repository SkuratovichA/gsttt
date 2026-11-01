import axios from './client';

declare const base: string;
declare const path: string;

axios.get(`${base}${path}`);
axios.get('/users');
