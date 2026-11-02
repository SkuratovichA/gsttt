const { client } = require('./client');
const { renamed } = require('./renamed');

client.get('/imported');
renamed.get('/imported');
