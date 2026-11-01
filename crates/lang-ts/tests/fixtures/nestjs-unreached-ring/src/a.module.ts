import { Module, forwardRef } from '@nestjs/common';
import { AController } from './a.controller';
import { BModule } from './b.module';

@Module({
  imports: [forwardRef(() => BModule)],
  controllers: [AController],
})
export class AModule {}
