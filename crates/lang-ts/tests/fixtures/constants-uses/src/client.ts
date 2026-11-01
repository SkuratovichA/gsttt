import axios from 'axios';

const API = 'https://api.example.com/v1';
const http = axios.create({ baseURL: API });

http.get('/x');
