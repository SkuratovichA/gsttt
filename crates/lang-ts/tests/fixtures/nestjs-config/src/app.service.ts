import { Injectable } from '@nestjs/common';
import { ConfigService } from '@nestjs/config';

@Injectable()
export class AppService {
  constructor(private readonly configService: ConfigService) {}

  load(configService: ConfigService, key: string) {
    const url = this.configService.get<string>('API_URL', 'http://localhost');
    const db = configService.getOrThrow('DB_URL');
    const host = this.configService.get('database.host');
    const computed = this.configService.get(key);
    return [url, db, host, computed];
  }
}
