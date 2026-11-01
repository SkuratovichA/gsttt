import { Module, forwardRef } from '@nestjs/common';
import { AuthModule } from '../auth/auth.module';
import { UserController } from './user.controller';

@Module({
  imports: [forwardRef(() => AuthModule)],
  controllers: [UserController],
})
export class UserModule {}
