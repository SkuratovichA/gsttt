import { ConfigService } from '@nestjs/config';
import { Env } from './env';

function read(configService: ConfigService, key: string) {
  return [
    configService.get('API_URL'),
    configService.get(Env.BASE),
    configService.get(key),
  ];
}

export const providers = [{ provide: ConfigService, useValue: { get: jest.fn() } }];
