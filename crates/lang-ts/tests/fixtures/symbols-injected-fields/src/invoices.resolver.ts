import { InvoiceService, MailQueue, JobQueue } from './services';

class InvoiceResolver {
  private readonly mail: MailQueue;
  private jobs = new JobQueue();
  constructor(
    private readonly invoices: InvoiceService,
    plain: InvoiceService,
  ) {}
  issue() {
    this.invoices.issue();
    this.mail.push();
    this.jobs.add();
  }
  other() {
    return this.plain;
  }
}
