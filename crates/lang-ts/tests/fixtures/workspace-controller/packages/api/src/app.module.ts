import { Module } from '@nestjs/common';
import { ItemsModule } from '@acme/items';

@Module({ imports: [ItemsModule] })
export class AppModule {}
