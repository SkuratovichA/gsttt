const { Router } = require('express');

const router = Router();
router.get('/:id', (req, res) => res.json({}));

module.exports = router;
