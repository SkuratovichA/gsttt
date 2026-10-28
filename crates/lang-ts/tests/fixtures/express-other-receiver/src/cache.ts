import express from 'express';

const cache = new Map<string, string>();
export const hit = cache.get('/health');

export const app = express();
