import { bare, dynamic, fixed } from './router';

function list() {}

dynamic.get('/users', list);
fixed.get('/orders', list);
bare.get('/items', list);
