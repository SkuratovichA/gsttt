import { NestFactory } from '@nestjs/core';
import { AppModule } from './app.module';

const P = 'api';

async function bootstrap() {
  const app = await NestFactory.create(AppModule);
  app.setGlobalPrefix(P);
  await app.listen(3000);
}

bootstrap();
