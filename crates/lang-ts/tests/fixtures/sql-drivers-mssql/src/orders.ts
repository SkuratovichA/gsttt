declare const sql: any;
declare const pool: any;

export const order = async (id: number) =>
  await pool.request().input('id', sql.Int, id).query('SELECT id FROM orders WHERE id = @id');

export const refund = () => new sql.Request().query('SELECT id FROM refunds');

export const cart = (id: number) => sql.query`SELECT id FROM carts WHERE id = ${id}`;
