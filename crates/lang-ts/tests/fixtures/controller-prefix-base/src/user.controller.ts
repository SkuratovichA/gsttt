import { Controller } from '@nestjs/common';
import { USERS } from './routes';
import { UserControllerBase } from './user.controller.base';

@Controller(USERS)
export class UserController extends UserControllerBase {}
