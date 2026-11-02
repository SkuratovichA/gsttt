import { Controller, Post } from '@nestjs/common';

@Controller('hooks')
export class HooksController {
  @Post([])
  receive() {}
}
