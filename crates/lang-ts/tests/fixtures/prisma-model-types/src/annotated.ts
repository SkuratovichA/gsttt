import { Tenant } from '@prisma/client';

declare function load<T>(): T;

export function rename(tenant: Tenant): Tenant {
  return tenant;
}

export const loaded = load<Tenant>();
