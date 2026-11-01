import { Controller, Get } from '@nestjs/common';

const BASE = 'users';

@Controller(BASE)
export class UsersController {
  @Get('x')
  find() {}
}
