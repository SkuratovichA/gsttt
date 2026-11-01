import { Controller, Get } from '@nestjs/common';

@Controller('legacy')
export class LegacyController {
  @Get(':id')
  find() {}
}
