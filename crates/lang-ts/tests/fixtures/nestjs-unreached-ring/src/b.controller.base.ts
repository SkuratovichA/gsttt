import { Get } from '@nestjs/common';

export class BControllerBase {
  @Get()
  list() {}
}
