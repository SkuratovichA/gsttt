import { Get, Post } from '@nestjs/common';

export class UserControllerBase {
  @Get(':id')
  find() {}

  @Post()
  create() {}
}
