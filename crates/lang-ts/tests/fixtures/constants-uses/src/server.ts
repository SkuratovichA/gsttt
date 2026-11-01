import express from 'express';

const MOUNT = '/v2';

const app = express();
const router = express.Router();

router.get('/y', (req, res) => res.json([]));
app.use(MOUNT, router);
