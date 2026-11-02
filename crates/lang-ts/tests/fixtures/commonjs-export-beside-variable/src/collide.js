const axios = require('axios');

const client = notAClient();
exports.client = axios.create({ baseURL: '/collide' });
client.get('/own-must-not-read');
