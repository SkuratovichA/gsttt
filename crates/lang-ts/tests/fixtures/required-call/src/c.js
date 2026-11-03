function load() {
  return require('axios').get('/c');
}

module.exports = { load };
