import { Controller, Get, Post } from '@nestjs/common';
import { LEGACY } from './paths';

@Controller(['a', 'b'])
export class ItemsController {
  @Get(LEGACY)
  list() {}

  @Post([LEGACY, 'x', LEGACY])
  create() {}
}
