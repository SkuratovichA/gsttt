const express = require('express');

const app = module.exports = express();
app.use('/mounted', require('./router'));
app.use('/both', require('./twice'));
