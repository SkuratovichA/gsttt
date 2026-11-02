import axios from 'axios';
import { API_URL } from './config';

const api = axios.create({ baseURL: API_URL });

api.get('/users');
api.post('/orders');
