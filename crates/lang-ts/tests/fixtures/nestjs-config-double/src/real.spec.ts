import { ConfigModule, ConfigService } from '@nestjs/config';
import { BillingService } from './billing';

const setup = {
  imports: [ConfigModule.forRoot()],
  providers: [ConfigService, { provide: BillingService, useValue: {} }],
};

export const url = (configService: ConfigService) => configService.get('API_URL');
