import { api } from './http';

type Http = { get(path: string): unknown };

export function list(api: Http) {
  return api.get('/users');
}

api.get('/orders');
