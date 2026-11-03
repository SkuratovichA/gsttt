import http from 'axios';

http.get('/a');
const api = http.create({ baseURL: '/v1' });
api.get('/d');
