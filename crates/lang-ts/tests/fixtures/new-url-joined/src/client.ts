export const one = (id: string) => fetch(new URL('users/' + id, 'https://api.example.com/v1/'));
export const all = () => fetch(new URL('users', 'https://api.example.com/v1'));
