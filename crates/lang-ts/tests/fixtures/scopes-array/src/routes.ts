import express from 'express';

const router = express.Router();

export function register() {
  const p = '/x';
  router.get(['/a', p], (req, res) => res.send('ok'));
}

export default router;
