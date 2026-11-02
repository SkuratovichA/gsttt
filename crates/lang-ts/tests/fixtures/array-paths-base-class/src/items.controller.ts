import { Controller } from '@nestjs/common';
import { ItemsControllerBase } from './items.controller.base';

@Controller(['a', 'b'])
export class ItemsController extends ItemsControllerBase {}
