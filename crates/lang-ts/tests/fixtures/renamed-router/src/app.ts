import ex, { Router as R } from 'express';

const app = ex();
app.get('/e', (req, res) => res.send('e'));
const router = R();
router.get('/f', (req, res) => res.send('f'));
