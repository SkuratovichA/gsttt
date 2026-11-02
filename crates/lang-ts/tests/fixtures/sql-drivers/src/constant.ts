declare const db: { query: (text: string) => unknown };

const LIST = 'SELECT id FROM accounts';

export const list = () => db.query(LIST);
