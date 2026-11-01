import { Controller } from '@nestjs/common';
import { UserControllerBase } from './user.controller.base';

@Controller('users')
export class UserController extends UserControllerBase {}
