import { Controller, Get } from '@nestjs/common';
import { paths } from './paths';

@Controller('/')
export class AuthController {
  @Get(paths.login())
  login() {}

  @Get()
  home() {}
}
