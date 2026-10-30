import { InvoiceService, MailQueue } from './services';

class Billing {
  constructor(private readonly svc: InvoiceService) {}
}

class Mailer {
  constructor(private readonly svc: MailQueue) {}
  send() {
    this.svc.push();
  }
}
