import { Get } from '@nestjs/common';

export class OrdersControllerBase {
  @Get()
  list() {}
}
