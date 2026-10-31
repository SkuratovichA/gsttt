import { Controller, Get } from '@nestjs/common';
import { LOGIN_PATH } from './paths';

@Controller('/')
export class AuthController {
  @Get(LOGIN_PATH)
  login() {}

  @Get()
  home() {}
}
