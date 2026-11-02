import { Get } from '@nestjs/common';

export class ItemsControllerBase {
  @Get('x')
  find() {}
}
