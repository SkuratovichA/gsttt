import express from 'express';

const app = express();

export function register(prefix: string) {
  const path = '/health';
  app.get(path, (req, res) => res.send('ok'));
  app.get(prefix, (req, res) => res.send('ok'));
}
