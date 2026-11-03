const router = require('express').Router();

router.get('/a', (req, res) => res.end());
router.post('/a', (req, res) => res.end());

module.exports = router;
