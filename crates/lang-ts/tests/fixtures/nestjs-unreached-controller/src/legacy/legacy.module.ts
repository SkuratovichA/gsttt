import { Module } from '@nestjs/common';
import { LegacyController } from './legacy.controller';

@Module({ controllers: [LegacyController] })
export class LegacyModule {}
