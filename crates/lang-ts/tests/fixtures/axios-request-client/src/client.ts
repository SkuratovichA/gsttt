import axios from 'axios';

const api = axios.create({ baseURL: '/api' });
export const drop = () => api.request({ url: '/users', method: 'delete' });
