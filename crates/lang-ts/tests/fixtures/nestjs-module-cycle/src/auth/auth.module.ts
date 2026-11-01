import { Module, forwardRef } from '@nestjs/common';
import { UserModule } from '../user/user.module';
import { AuthController } from './auth.controller';

@Module({
  imports: [forwardRef(() => UserModule)],
  controllers: [AuthController],
})
export class AuthModule {}
