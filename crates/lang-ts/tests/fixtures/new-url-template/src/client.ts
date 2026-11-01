export const users = (baseUrl: string) => fetch(new URL(`${baseUrl}/users`));
export const orders = (API: string) => fetch(new URL(`${API}/orders`, window.location.origin));
