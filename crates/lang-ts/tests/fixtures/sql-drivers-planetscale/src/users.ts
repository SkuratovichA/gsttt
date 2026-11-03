declare const conn: any;

export const user = (id: number) => conn.execute('SELECT id FROM users WHERE id = ?', [id]);
