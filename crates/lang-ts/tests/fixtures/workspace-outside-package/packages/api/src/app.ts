import express from 'express';
import { shared } from '@outside/routes';

const app = express();
app.use('/ext', shared);
app.get('/own', (req, res) => res.end());
