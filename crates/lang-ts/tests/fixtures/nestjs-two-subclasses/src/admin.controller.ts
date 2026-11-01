import { Controller } from '@nestjs/common';
import { UserControllerBase } from './user.controller.base';

@Controller('admin/users')
export class AdminController extends UserControllerBase {}
