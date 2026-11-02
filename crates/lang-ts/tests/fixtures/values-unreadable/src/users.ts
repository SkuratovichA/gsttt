import axios from 'axios';
import { BASE } from './env';
import { PATH } from './config';

fetch(`${BASE}/users`);
fetch(BASE);
fetch(PATH);

const api = axios.create({ baseURL: BASE });
api.get('/orders');
