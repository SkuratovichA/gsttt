const { makeService } = require('./service');
const lib = require('./service');

function named() { makeService().place(); }
function namespaced() { lib.makeService().place(); }
module.exports = { named, namespaced };
