declare const pool: any;
declare const build: () => string;

export const invoices = () => pool.query({ text: 'SELECT id FROM invoices', values: [1] });

export const built = () => pool.query({ text: build() });
