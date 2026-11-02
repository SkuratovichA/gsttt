declare const db: { query: (text: string) => unknown };

export const find = () => db.query('SELECT id FROM users');
