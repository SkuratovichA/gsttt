const axios = require('axios');

const httpClient = axios.create({ baseURL: '/api' });

module.exports = { httpClient };
