import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const environmentTronInitialized = z.null();

export const environmentTronInitializedEvent = defineEvent(
  'Environment.TronInitialized',
  environmentTronInitialized,
);
