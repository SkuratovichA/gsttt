import { Controller, Get } from '@nestjs/common';

@Controller({ version: '1' })
export class HealthController {
  @Get('health')
  check() {}
}
