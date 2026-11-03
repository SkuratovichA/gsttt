import express from 'express';
import { API_PREFIX } from '@acme/shared';
import { users } from './users';

const app = express();
app.use(API_PREFIX, users);
