const { client: named } = require('./exports-client');
const { client: member } = require('./module-exports-client');
const { client: inObject } = require('./object-client');

named.get('/from-exports');
member.get('/from-module-exports');
inObject.get('/from-object');
