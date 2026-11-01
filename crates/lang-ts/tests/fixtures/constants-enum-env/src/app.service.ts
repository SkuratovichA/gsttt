import { Injectable } from '@nestjs/common';
import { ConfigService } from '@nestjs/config';
import { Env } from './env';

@Injectable()
export class AppService {
  constructor(private readonly configService: ConfigService) {}

  url() {
    return this.configService.get(Env.API_URL);
  }
}
