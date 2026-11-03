declare const headers: Record<string, string>;

export const plain = () => fetch('/x');
export const empty = () => fetch('/x', {});
export const withHeaders = () => fetch('/x', { headers });
export const put = () => fetch('/x', { method: 'PUT' });
