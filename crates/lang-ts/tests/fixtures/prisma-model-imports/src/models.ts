import type {
  Tenant,
  TenantBranch as Branch,
  Prisma,
  PrismaClient,
} from '@prisma/client';
import { BookingStatus } from '@prisma/client';

export type Pair = [Tenant, Branch, Prisma.TenantWhereInput, PrismaClient, BookingStatus];
