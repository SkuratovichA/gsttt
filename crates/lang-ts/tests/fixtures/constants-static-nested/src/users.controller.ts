import { Controller, Get } from '@nestjs/common';
import { ROUTES, Routes } from './routes';

@Controller('users')
export class UsersController {
  @Get(Routes.LOGIN)
  login() {}

  @Get(ROUTES.users.byId)
  find() {}
}
