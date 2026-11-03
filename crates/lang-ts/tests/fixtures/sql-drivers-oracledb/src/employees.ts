declare const connection: any;

export const employee = (id: number) => connection.execute('SELECT id FROM employees WHERE id = :id', [id]);
