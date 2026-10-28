import { Controller, Get } from '@nestjs/common';

@Controller('bookings')
export class BookingsController {
  @Get(':id')
  find() {}
}
