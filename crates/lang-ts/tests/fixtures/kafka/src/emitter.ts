import { EventEmitter2 } from '@nestjs/event-emitter';

export class Notifier {
  constructor(private readonly emitter: EventEmitter2) {}

  notify(token: string) {
    this.emitter.emit('change', token);
  }
}
