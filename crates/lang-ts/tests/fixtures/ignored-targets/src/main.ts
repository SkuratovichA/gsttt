import { client } from '../generated/client';
import { viaAlias } from '@gen/client';
import { absent } from './absent';
import { folder } from './folder';

export const main = [client, viaAlias, absent, folder];
