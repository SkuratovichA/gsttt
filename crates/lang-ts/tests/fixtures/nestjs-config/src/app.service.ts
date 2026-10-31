import { Injectable } from '@nestjs/common';
import { ConfigService } from '@nestjs/config';
import { Env } from './env';

@Injectable()
export class AppService {
  constructor(private readonly configService: ConfigService) {}

  load(configService: ConfigService) {
    const url = this.configService.get<string>('API_URL', 'http://localhost');
    const db = configService.getOrThrow('DB_URL');
    const host = this.configService.get('database.host');
    const computed = this.configService.get(Env.API_URL);
    return [url, db, host, computed];
  }
}
