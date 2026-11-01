import { Controller } from '@nestjs/common';
import { prefix } from './routes';
import { UserControllerBase } from './user.controller.base';

@Controller(prefix())
export class UserController extends UserControllerBase {}
