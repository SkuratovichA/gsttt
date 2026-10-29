import { PrismaClient } from '@prisma/client';

class Audited extends PrismaClient {}

class Logged extends Audited {}

interface Narrow extends Pick<PrismaClient, 'tenant'> {}

interface Wide extends Narrow {}

export function audited(client: Audited) {
  return client.tenant.findFirst();
}

export function logged(logger: Logged) {
  return logger.tenant.findFirst();
}

export function narrow(slice: Narrow) {
  return slice.tenant.findFirst();
}

export function wide(whole: Wide) {
  return whole.tenant.findFirst();
}

const made = new Logged();

export function count() {
  return made.tenant.count();
}
