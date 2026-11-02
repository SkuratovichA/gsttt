import { ClientKafka } from '@nestjs/microservices';

export class Publisher {
  constructor(private readonly kafka: ClientKafka) {}

  publish(user: string) {
    this.kafka.emit('user.created', user);
  }
}
