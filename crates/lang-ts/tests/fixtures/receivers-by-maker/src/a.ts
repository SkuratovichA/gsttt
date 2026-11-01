import { app } from './app';
import { api } from './http';

const params = { page: 1 };
app.get('env');
app.get('/k');
api.get('/users', { params });
