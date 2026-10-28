import express from 'express';

const app = express();
app.get('/health', (req, res) => res.send('ok'));

export default app;
