import axios from 'axios';

export const book = () => axios.request({ url: '/bookings', method: 'POST' });
export const all = () => axios.request({ url: '/users' });
