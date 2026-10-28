import { Router } from 'express';
import bookings from './bookings';

const api = Router();
api.use('/bookings', bookings);

export default api;
