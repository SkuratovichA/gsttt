import * as common from '@nestjs/common';

export class UserControllerBase {
  @common.Get('/:id')
  find() {}
}
