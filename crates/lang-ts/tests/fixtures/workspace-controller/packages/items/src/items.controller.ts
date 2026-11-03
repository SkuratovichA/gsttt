import { Controller, Get } from '@nestjs/common';

@Controller('items')
export class ItemsController {
  @Get(':id')
  find() {}
}
