import express, { Router } from 'express';

const app = express();
const router = Router();

app.use('/api', router);
app.use(router);
router.get('/x', (req, res) => res.json({}));
