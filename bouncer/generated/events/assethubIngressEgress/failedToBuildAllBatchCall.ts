import { z } from 'zod';
import { cfChainsAllBatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const assethubIngressEgressFailedToBuildAllBatchCall = z.object({
  error: cfChainsAllBatchError,
});

export const assethubIngressEgressFailedToBuildAllBatchCallEvent = defineEvent(
  'AssethubIngressEgress.FailedToBuildAllBatchCall',
  assethubIngressEgressFailedToBuildAllBatchCall,
);
