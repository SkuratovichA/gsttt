import type { Other } from 'other-lib';

export function load(client: Pick<Other, 'tenant'>) {
  return client.tenant.findFirst();
}
