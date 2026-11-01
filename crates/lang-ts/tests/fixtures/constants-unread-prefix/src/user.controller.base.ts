import { Get } from '@nestjs/common';

export class UserControllerBase {
  @Get(':id')
  find() {}
}
