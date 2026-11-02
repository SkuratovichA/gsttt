import { Controller, Get } from '@nestjs/common';
import { USERS } from './routes';

@Controller(USERS)
export class UsersController {
  @Get('x')
  x() {}
}
