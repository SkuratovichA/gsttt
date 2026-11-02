import { Controller, Post } from '@nestjs/common';
import { dynamic } from './paths';

@Controller('hooks')
export class HooksController {
  @Post(['webhook', dynamic(), 'webhook/:token'])
  receive() {}
}
