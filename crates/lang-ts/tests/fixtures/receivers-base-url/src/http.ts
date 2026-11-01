import axios from 'axios';
import { API } from './config';

export const api = axios.create({ baseURL: API });
