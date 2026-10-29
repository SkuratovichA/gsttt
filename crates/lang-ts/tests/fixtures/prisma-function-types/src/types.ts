import type { PrismaClient } from '@prisma/client';

export function make(): (returnedClient: PrismaClient) => void {
  return () => undefined;
}

export class Holder {
  handler?: (propertyClient: PrismaClient) => void;
}

export const cast = make() as (castClient: PrismaClient) => void;

export const reads = [
  returnedClient.tenant.findFirst(),
  propertyClient.tenant.findFirst(),
  castClient.tenant.findFirst(),
];
