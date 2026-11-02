import { Controller, Get } from '@nestjs/common';
import { LOGIN, USERS } from './routes';

@Controller(USERS)
export class UsersController {
  @Get(LOGIN)
  login() {}
}
