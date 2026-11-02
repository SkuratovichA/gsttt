const axios = require('axios');

module.exports = { client: axios.create({ baseURL: '/in-object' }) };
