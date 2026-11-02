import { PrismaService } from './prisma.service';

export class Reports {
  constructor(private readonly prisma: PrismaService, private readonly db: { client: PrismaService }) {}

  recent() {
    return this.prisma.$queryRaw`SELECT id FROM reports`;
  }

  deep() {
    return this.db.client.$queryRaw`SELECT id FROM audits`;
  }
}
