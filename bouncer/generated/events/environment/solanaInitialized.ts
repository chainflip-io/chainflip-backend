import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const environmentSolanaInitialized = z.null();

export const environmentSolanaInitializedEvent = defineEvent(
  'Environment.SolanaInitialized',
  environmentSolanaInitialized,
);
