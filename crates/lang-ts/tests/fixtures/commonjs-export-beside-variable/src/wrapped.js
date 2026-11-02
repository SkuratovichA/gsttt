const express = require('express');

const router = express.Router();
router.get('/wrapped-route', h);
exports.router = wrap(router);
