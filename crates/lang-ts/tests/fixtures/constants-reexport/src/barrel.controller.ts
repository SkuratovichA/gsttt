import { Controller, Get } from '@nestjs/common';
import { LOGIN_PATH } from './paths';

@Controller('/')
export class BarrelController {
  @Get(LOGIN_PATH)
  login() {}
}
