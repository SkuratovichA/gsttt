import express from 'express';

const app = express();
app.get(['/a', '/b'], (req, res) => res.send('ok'));

export default app;
