import { Controller, Get } from '@nestjs/common';
import { OTHER, USERS } from './routes';

@Controller([USERS, OTHER])
export class UsersController {
  @Get('x')
  x() {}
}
