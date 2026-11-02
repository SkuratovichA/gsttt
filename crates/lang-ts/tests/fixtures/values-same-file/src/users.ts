import { BASE } from './config';

const USERS = `${BASE}/users`;
const ORDERS = BASE + '/orders';
const ALIAS = BASE;

fetch(USERS);
fetch(ORDERS);
fetch(`${USERS}/1`);
fetch(ALIAS);
