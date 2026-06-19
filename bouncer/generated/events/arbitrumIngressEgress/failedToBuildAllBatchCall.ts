import { z } from 'zod';
import { cfChainsAllBatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressFailedToBuildAllBatchCall = z.object({
  error: cfChainsAllBatchError,
});

export const arbitrumIngressEgressFailedToBuildAllBatchCallEvent = defineEvent(
  'ArbitrumIngressEgress.FailedToBuildAllBatchCall',
  arbitrumIngressEgressFailedToBuildAllBatchCall,
);
