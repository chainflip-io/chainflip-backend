import { z } from 'zod';
import { defineEvent } from '@chainflip/processor/event';

export const environmentNonNativeSignedCall = z.null();

export const environmentNonNativeSignedCallEvent = defineEvent(
  'Environment.NonNativeSignedCall',
  environmentNonNativeSignedCall,
);
