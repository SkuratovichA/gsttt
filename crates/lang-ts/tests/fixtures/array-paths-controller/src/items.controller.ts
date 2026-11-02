import { Controller, Get } from '@nestjs/common';

@Controller(['a', 'b'])
export class ItemsController {
  @Get('x')
  one() {}

  @Get(['y', 'z'])
  two() {}
}
