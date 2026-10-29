import type { PrismaClient } from '@prisma/client';

export interface Deps {
  run(methodClient: PrismaClient): void;
  (callClient: PrismaClient): void;
  new (constructClient: PrismaClient): Deps;
}

export type Runner = (typeClient: PrismaClient) => void;

export declare function declared(declareClient: PrismaClient): void;

export function overloaded(overloadClient: PrismaClient): void;
export function overloaded(value: unknown): void {
  void value;
}

export abstract class Base {
  abstract run(abstractClient: PrismaClient): void;
}

export const reads = [
  methodClient.tenant.findFirst(),
  callClient.tenant.findFirst(),
  constructClient.tenant.findFirst(),
  typeClient.tenant.findFirst(),
  declareClient.tenant.findFirst(),
  overloadClient.tenant.findFirst(),
  abstractClient.tenant.findFirst(),
];
