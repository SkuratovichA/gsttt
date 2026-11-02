const axios = require('axios');

const client = exports.client = axios.create({ baseURL: '/both' });
client.get('/own');
