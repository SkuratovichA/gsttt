import express from 'express';

const app = express();
const env = app.get('env');
app.get('/ping', (req, res) => res.json({ env }));
