import { Module } from '@nestjs/common';
import { UserModule } from './user/user.module';

@Module({ imports: [UserModule] })
export class AppModule {
  static forRoot() {
    return { module: AppModule };
  }
}
