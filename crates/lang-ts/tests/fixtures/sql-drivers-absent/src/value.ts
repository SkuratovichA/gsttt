declare const getDb: () => any;
declare const sql: any;
declare const id: number;

export const audits = () => getDb().query('SELECT id FROM audits');

export const cart = () => sql`SELECT id FROM carts WHERE id = ${id}`;
