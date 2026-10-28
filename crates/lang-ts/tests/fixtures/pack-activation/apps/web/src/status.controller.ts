import { Controller, Get } from '@nestjs/common';

@Controller('status')
export class StatusController {
  @Get('live')
  live() {}
}
