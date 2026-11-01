import { Controller, Get } from '@nestjs/common';
import { LOGIN, PREFIX } from './routes';

@Controller(PREFIX)
export class AuthController {
  @Get('login')
  login() {}

  @Get(LOGIN)
  again() {}
}
