const axios = require('axios');

exports.client = axios.create({ baseURL: '/named' });
