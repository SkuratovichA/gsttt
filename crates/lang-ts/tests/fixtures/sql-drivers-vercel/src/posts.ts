declare const sql: any;
declare const db: any;

export const users = () => sql`SELECT id FROM users`;

export const posts = () => db.sql`SELECT id FROM posts`;
