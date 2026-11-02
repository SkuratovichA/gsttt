declare const GET_USERS: unknown;
declare const client: { query: (options: { query: unknown }) => unknown };
declare const pool: { query: (text: string) => unknown };

export const users = () => client.query({ query: GET_USERS });

export const run = (text: string) => pool.query(text);
