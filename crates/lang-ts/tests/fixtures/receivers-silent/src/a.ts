import { api } from './api';
import { cache } from './cache';
import * as store from './cache';

cache.get('k');
store.cache.get('k');
api.get('/users');
