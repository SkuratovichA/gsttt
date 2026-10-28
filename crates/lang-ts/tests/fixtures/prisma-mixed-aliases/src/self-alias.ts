import { PrismaClient } from '@prisma/client';

export class TenantService {
  prisma = new PrismaClient();

  find() {
    const self = this;
    return self.prisma.tenant.findFirst();
  }
}

export function other(self: Mock) {
  return self.prisma.tenant.findMany();
}
