import { PrismaClient } from '@prisma/client';

export class TenantService {
  prisma = new PrismaClient();

  find(id: number) {
    const self = this;
    return [id].map(function () {
      return self.prisma.tenant.findUnique({ where: { id } });
    });
  }

  total() {
    let that = this;
    return that.prisma.booking.count();
  }
}

export function unrelated(shared: { prisma: PrismaClient }) {
  const holder = shared;
  return holder.prisma.invoice.findMany();
}
