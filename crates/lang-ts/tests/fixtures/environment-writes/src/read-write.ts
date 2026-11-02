export const url = (process.env.API_URL ??= 'http://localhost');
export const retries = () => (process.env.RETRIES ??= '3');
export const next = ++process.env.COUNT;
