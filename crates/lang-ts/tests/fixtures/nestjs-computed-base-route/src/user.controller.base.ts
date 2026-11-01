import { Get } from '@nestjs/common';
import { ROUTES } from './routes';

export class UserControllerBase {
  @Get(ROUTES.USER)
  find() {}
}
