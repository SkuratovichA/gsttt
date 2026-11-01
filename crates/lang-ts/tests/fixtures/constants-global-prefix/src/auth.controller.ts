import { Controller, Get } from '@nestjs/common';
import { LOGIN_PATH } from './paths';

@Controller('auth')
export class AuthController {
  @Get(LOGIN_PATH)
  login() {}
}
