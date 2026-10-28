import { PrismaClient } from '@prisma/client';

export class Both {
  prisma = new PrismaClient();

  first() {
    const self = this;
    return self.prisma.tenant.findFirst();
  }

  second() {
    let self = this;
    return self.prisma.booking.count();
  }
}
