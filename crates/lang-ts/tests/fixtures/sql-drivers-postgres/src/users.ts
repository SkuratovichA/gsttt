declare const sql: any;

export const user = (id: number) => sql`SELECT id FROM users WHERE id = ${id}`;
