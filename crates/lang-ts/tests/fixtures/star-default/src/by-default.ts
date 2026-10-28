import express from 'express';
import fallback from './routes';

const app = express();
app.use('/old', fallback);
