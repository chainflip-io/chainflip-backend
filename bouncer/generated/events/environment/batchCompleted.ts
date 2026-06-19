import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const environmentBatchCompleted = z.null();

export const environmentBatchCompletedEvent = defineEvent(
  'Environment.BatchCompleted',
  environmentBatchCompleted,
);
