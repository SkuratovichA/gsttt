declare const options: RequestInit;
declare const verb: string;

export const shared = () => fetch('/x', options);
export const named = () => fetch('/x', { method: verb });
export const spread = () => fetch('/x', { ...options });
