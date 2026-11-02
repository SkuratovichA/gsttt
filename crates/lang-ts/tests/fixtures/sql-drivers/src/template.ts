declare const pool: { query: (text: string) => unknown };

export const order = (id: number) => pool.query(`SELECT id FROM orders WHERE id = ${id}`);

export const any = (table: string) => pool.query(`SELECT id FROM ${table}`);
