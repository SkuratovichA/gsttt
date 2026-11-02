const axios = require('axios');

const made = exports.renamed = axios.create({ baseURL: '/renamed' });
made.get('/own');
