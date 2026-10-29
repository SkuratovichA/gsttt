import type { PrismaClient } from '@prisma/client';

type Ping = Pong;
type Pong = Ping;
type Back = Forth | PrismaClient;
type Forth = Back;

export function loop(ring: Ping) {
  return ring.tenant.findMany();
}

export function escape(value: Forth) {
  return value.booking.count();
}
