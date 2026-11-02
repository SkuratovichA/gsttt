export class Bookings {
  constructor(private readonly prisma: PrismaService) {}

  create(data: unknown) {
    return this.prisma.$transaction(function (trx) {
      return trx.booking.create({ data });
    });
  }
}
