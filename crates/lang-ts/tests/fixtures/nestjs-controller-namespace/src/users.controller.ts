import * as common from '@nestjs/common';

@common.Controller('users')
export class UsersController {
  @common.Get('/:id')
  find() {}

  @common.Post()
  create() {}
}
