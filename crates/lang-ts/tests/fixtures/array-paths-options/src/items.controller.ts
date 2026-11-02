import { Controller, Get } from '@nestjs/common';

@Controller({ path: ['a', 'b'] })
export class ItemsController {
  @Get('x')
  one() {}
}
