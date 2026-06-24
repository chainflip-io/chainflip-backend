import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const environmentBscInitialized = z.null();

export const environmentBscInitializedEvent = defineEvent(
  'Environment.BscInitialized',
  environmentBscInitialized,
);
