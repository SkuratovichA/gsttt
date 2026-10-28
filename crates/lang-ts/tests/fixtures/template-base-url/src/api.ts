declare const API_URL: string;

export const list = () => fetch(`${API_URL}/bookings`);
