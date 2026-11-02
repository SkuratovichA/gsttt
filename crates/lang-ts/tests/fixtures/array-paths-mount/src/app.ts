import express from 'express';

const app = express();
const router = express.Router();
router.get('/x', (req, res) => res.send('ok'));
app.use(['/a', '/b'], router);
app.get(['/a', '/b'], (req, res) => res.send('ok'));

export default app;
