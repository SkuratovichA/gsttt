import { api } from '../utils/api';

export function Bookings({ id }: { id: string }) {
  const booking = api.booking.get.useQuery({ id });
  return <div>{booking.data?.id}</div>;
}
