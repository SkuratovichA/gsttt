import { Controller } from '@nestjs/common';
import { BControllerBase } from './b.controller.base';

@Controller('b')
export class BController extends BControllerBase {}
