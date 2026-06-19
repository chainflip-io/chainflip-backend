import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const environmentArbitrumInitialized = z.null();

export const environmentArbitrumInitializedEvent = defineEvent(
  'Environment.ArbitrumInitialized',
  environmentArbitrumInitialized,
);
