import * as common from '@nestjs/common';
import { UserControllerBase } from './base/user.controller.base';

@common.Controller('users')
export class UserController extends UserControllerBase {}
