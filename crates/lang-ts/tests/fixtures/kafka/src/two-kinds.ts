import { Producer } from 'kafkajs';
import { EventEmitter2 } from '@nestjs/event-emitter';

export function kafka(client: Producer) {
  client.send({ topic: 'x', messages: [] });
}

export function local(client: EventEmitter2) {
  client.emit('y', 1);
}
