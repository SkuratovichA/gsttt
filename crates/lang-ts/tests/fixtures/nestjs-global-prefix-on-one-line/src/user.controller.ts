import { Controller, Get } from '@nestjs/common';

@Controller('users')
export class UserController {
  @Get(':id') find() { return fetch('/c'); }
}
