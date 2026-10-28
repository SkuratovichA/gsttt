import { Mutation, Query, Resolver } from '@nestjs/graphql';

class Booking {}

@Resolver()
export class BookingsResolver {
  @Query(() => [Booking])
  bookings() {}

  @Mutation(() => Booking, { name: 'cancelBooking' })
  cancel() {}
}
