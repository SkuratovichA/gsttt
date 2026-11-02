import axios from 'axios';

declare const id: string;
declare const BASE: string;
const bare = axios.create();
export const page = () => fetch('items');
export const plain = () => axios.get('users');
export const made = () => bare.get('users/me');
export const template = () => fetch(`items/${id}`);
export const climbing = () => fetch('../items');
export const data = () => fetch('data:text/plain,x');
export const query = () => fetch('?x=1');
export const absolute = () => fetch('https://a.test/x');
export const dotted = () => fetch(`./${id}`);
export const nested = () => fetch(`./${id}/y`);
const unread = axios.create({ baseURL: BASE });
export const behind = () => unread.get('users');
