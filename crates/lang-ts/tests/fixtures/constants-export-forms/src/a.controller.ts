import { Controller, Get } from '@nestjs/common';
import HOME_PATH, { LOGIN_PATH } from './lib';

@Controller('/')
export class AController {
  @Get(LOGIN_PATH)
  login() {}

  @Get(HOME_PATH)
  home() {}
}
