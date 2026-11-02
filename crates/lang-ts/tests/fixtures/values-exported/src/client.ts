import { ORDERS, PATHS, USERS } from './routes';

fetch(USERS);
fetch(ORDERS);
fetch(PATHS.items);
fetch(`${USERS}/1`);
