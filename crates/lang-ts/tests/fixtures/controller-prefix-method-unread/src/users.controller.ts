import { Controller, Get } from '@nestjs/common';
import { ADMIN, USERS } from './routes';

@Controller([USERS, ADMIN])
export class UsersController {
  @Get(compute())
  computed() {}

  @Get('x')
  x() {}
}
