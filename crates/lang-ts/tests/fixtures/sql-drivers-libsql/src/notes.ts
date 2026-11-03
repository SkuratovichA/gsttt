declare const client: any;

export const notes = () => client.execute('SELECT id FROM notes');

export const tag = (id: number) => client.execute({ sql: 'SELECT id FROM tags WHERE id = ?', args: [id] });
