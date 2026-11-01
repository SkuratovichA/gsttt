import { Controller } from '@nestjs/common';
import { OrdersControllerBase } from './orders.controller.base';

@Controller('orders')
export class OrdersController extends OrdersControllerBase {}
