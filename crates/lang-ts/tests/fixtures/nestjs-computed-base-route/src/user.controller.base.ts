import { Get } from '@nestjs/common';
import { route } from './routes';

export class UserControllerBase {
  @Get(route('user'))
  find() {}
}
