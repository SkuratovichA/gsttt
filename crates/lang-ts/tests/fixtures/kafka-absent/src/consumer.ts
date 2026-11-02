import { EventPattern, MessagePattern } from '@nestjs/microservices';

export class Users {
  @EventPattern('user.created')
  created() {}

  @MessagePattern('user.get')
  get() {}
}
