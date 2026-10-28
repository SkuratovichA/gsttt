import express from 'express';

const app = express();
app.get('/health', (req, res) => res.json({}));

export default app;
