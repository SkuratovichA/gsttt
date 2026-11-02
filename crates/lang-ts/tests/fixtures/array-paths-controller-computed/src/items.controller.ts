import { Controller, Get } from '@nestjs/common';
import { prefix } from './paths';

@Controller(['a', prefix()])
export class ItemsController {
  @Get('x')
  one() {}
}
