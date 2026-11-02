import { Controller, Get } from '@nestjs/common';
import { BASE, Paths, ROUTES, USERS } from './routes';

@Controller(ROUTES.orders)
export class OrdersController {
  @Get('a')
  a() {}
}

@Controller(Paths.ITEMS)
export class ItemsController {
  @Get('b')
  b() {}
}

@Controller({ path: USERS })
export class UsersController {
  @Get('c')
  c() {}
}

@Controller(`${BASE}/v1`)
export class VersionedController {
  @Get('d')
  d() {}
}
