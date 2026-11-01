import * as common from '@nestjs/common';
import { UserControllerBase } from './user.controller.base';

@common.Controller('users')
export class UserController extends UserControllerBase {}
