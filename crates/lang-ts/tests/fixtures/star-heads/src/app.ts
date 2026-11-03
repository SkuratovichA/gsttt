import express from 'express';
import { members } from './routes-barrel';
import fallback from './routes-barrel';
import direct from './routes';

const app = express();
app.use('/members', members);
app.use('/lost', fallback);
app.use('/direct', direct);
