import { Module, forwardRef } from '@nestjs/common';
import { AModule } from './a.module';
import { BController } from './b.controller';

@Module({
  imports: [forwardRef(() => AModule)],
  controllers: [BController],
})
export class BModule {}
