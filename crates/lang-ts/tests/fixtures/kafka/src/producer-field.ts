import { Inject } from '@nestjs/common';
import { ClientKafka } from '@nestjs/microservices';

export class Events {
  @Inject('KAFKA') private readonly kafka: ClientKafka;

  publish() {
    this.kafka.emit('user.deleted', {});
  }
}
