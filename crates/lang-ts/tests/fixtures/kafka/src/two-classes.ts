import { ClientKafka } from '@nestjs/microservices';
import { EventEmitter2 } from '@nestjs/event-emitter';

export class KafkaSender {
  constructor(private readonly sender: ClientKafka) {}

  publish() {
    this.sender.emit('z', 1);
  }
}

export class LocalSender {
  constructor(private readonly sender: EventEmitter2) {}

  publish() {
    this.sender.emit('w', 1);
  }
}
