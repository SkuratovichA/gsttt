declare const db: { query: (text: string, values?: unknown[]) => unknown };

export const find = (id: number) =>
  db.query('SELECT id FROM users WHERE id = $1', [id]);

export class Sessions {
  constructor(private pool: { query: (text: string) => unknown }) {}
  clear() { return this.pool.query('DELETE FROM sessions'); }
}
