import { Controller, Post } from '@nestjs/common';
import { LEGACY } from './paths';

@Controller('hooks')
export class HooksController {
  @Post(['webhook', LEGACY])
  receive() {}
}
