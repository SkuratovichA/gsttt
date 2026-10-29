import type { PrismaClient } from '@prisma/client';

export interface Deps {
  run(methodClient: string): void;
  (callClient: string): void;
  new (constructClient: string): Deps;
}

export type Runner = (typeClient: string) => void;

export declare function declared(declareClient: string): void;

export function overloaded(overloadClient: string): void;
export function overloaded(value: unknown): void {
  void value;
}

export abstract class Base {
  abstract run(abstractClient: string): void;
}

export function make(): (returnedClient: string) => void {
  return () => undefined;
}

export function pick<typeParameterClient>(value: typeParameterClient): typeParameterClient {
  return value;
}

export function reads(
  methodClient: PrismaClient,
  callClient: PrismaClient,
  constructClient: PrismaClient,
  typeClient: PrismaClient,
  declareClient: PrismaClient,
  overloadClient: PrismaClient,
  abstractClient: PrismaClient,
  returnedClient: PrismaClient,
  typeParameterClient: PrismaClient,
) {
  return [
    methodClient.tenant.findFirst(),
    callClient.tenant.findFirst(),
    constructClient.tenant.findFirst(),
    typeClient.tenant.findFirst(),
    declareClient.tenant.findFirst(),
    overloadClient.tenant.findFirst(),
    abstractClient.tenant.findFirst(),
    returnedClient.tenant.findFirst(),
    typeParameterClient.tenant.findFirst(),
  ];
}
