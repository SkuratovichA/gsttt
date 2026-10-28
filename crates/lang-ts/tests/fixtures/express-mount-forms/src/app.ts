import express from 'express';
import { users } from './users';
import { extras } from './extras';
import { pair } from './pair';
import { mixed } from './mixed';

const app = express();
const dynamic = '/dyn';

app.use('/v1', users);
app.use('/v2', users);
app.use(extras);
app.use(['/a', '/b'], pair);
app.use(['/c', dynamic], mixed);
