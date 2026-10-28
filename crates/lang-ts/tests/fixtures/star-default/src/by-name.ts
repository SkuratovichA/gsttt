import express from 'express';
import { users } from './routes';

const app = express();
app.use('/users', users);
