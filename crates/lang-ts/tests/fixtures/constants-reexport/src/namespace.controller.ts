import { Controller, Get } from '@nestjs/common';
import * as paths from './paths/auth';

@Controller('/')
export class NamespaceController {
  @Get(paths.LOGIN_PATH)
  login() {}
}
