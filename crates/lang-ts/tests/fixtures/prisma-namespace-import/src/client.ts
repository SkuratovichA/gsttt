import * as client from '@prisma/client';
import clientDefault from '@prisma/client';
import { default as other } from '@prisma/client';
import '@prisma/client';

export * from '@prisma/client';
export * as models from '@prisma/client';
export { default as Client } from '@prisma/client';

export const all = [client, clientDefault, other];
