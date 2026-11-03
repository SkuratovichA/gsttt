const { Router: R } = require('express');
const router = R();
router.get('/f', (req, res) => res.send('f'));
