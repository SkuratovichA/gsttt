import express from 'express';
import { users } from './a';

const app = express();
app.use('/users', users);
