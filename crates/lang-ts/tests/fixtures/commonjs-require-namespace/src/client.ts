import axios from 'axios';

export default axios.create({ baseURL: '/real' });
export const get = axios.create({ baseURL: '/other' });
