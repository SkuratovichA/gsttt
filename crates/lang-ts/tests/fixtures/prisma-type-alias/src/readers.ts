import type { Prisma } from '@prisma/client';

type Client = Prisma.TransactionClient;
type Reader = Pick<Client, 'tenant'>;
type Options = { tenant: { findFirst(): void } };
type Loop = Again;
type Again = Loop;

export function load(reader: Reader, id: number) {
  return reader.tenant.findUnique({ where: { id } });
}

export function pick(options: Options) {
  return options.tenant.findFirst();
}

export function spin(loop: Loop) {
  return loop.tenant.findMany();
}
