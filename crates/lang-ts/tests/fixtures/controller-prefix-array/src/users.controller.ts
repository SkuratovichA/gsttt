import { Controller, Get } from '@nestjs/common';
import { USERS } from './routes';

@Controller([USERS, 'literal'])
export class UsersController {
  @Get('x')
  x() {}
}
