import { NestFactory } from '@nestjs/core';
import { AppModule } from './app.module';

async function bootstrap() {
  const app = await NestFactory.create(AppModule);
  const prefix = ['api', 'v1'].join('/');
  app.setGlobalPrefix(prefix);
  await app.listen(3000);
}

bootstrap();
