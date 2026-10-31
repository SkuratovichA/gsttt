declare const API_URL: string;
declare const base: string;

export const one = (id: string) => fetch(`${API_URL}/${id}`);

export const two = (period: string, name: string) => fetch(`${base}${period}/${name}`);

export const three = (id: string) => fetch(`${API_URL}/users/${id}`);
