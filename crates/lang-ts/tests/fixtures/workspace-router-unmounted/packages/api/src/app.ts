import express from 'express';
import { shared } from '@acme/routes';

const app = express();
app.get('/own', (req, res) => res.end());
export const unused = shared;
