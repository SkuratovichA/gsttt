import axios from 'axios';

const api = axios.create({ baseURL: '/b' });

export default api;
