import * as services from './services';
import { JobQueue, MailQueue } from './queues';

class Local {
  run() {}
  stop() {}
}

class Notifier {
  constructor(
    private readonly mail: services.MailQueue,
    private readonly either: MailQueue | JobQueue,
    private readonly local: Local,
  ) {}
  qualified() {
    this.mail.push();
  }
  union() {
    this.either.send();
  }
  declared() {
    this.local.run();
  }
  undeclared() {
    this.local.start();
  }
  nested() {
    this.mail.items.push();
  }
  indexed(key: string) {
    return this.mail[key];
  }
}
