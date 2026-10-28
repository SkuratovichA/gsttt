import express, { Router } from 'express';

const app = express();
const router = Router();
const h = (req, res) => res.json({});

router.get('/a', h); router.post('/b', h);
app.use('/api', router);
