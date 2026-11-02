import { Test } from '@nestjs/testing';
import { ConfigService } from '@nestjs/config';
import { AppModule } from './app.module';

const module = Test.createTestingModule({ imports: [AppModule] })
  .overrideProvider(ConfigService)
  .useValue({ get: jest.fn() });

export const url = (configService: ConfigService) => configService.get('API_URL');
