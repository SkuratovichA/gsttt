import axios from 'axios';

export async function cancel(id: string, body: unknown) {
  await fetch(`/bookings/${id}`, { method: 'DELETE' });
  return axios.post('/bookings', body);
}
