import { ConfigService } from '@nestjs/config';

export const providers = [{ provide: ConfigService, useValue: {} }];

export const url = (configService: ConfigService) => configService.get('API_URL');
