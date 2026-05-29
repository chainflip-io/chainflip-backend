import { z } from 'zod';
import { cfChainsAllBatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressFailedToBuildAllBatchCall = z.object({
  error: cfChainsAllBatchError,
});

export const ethereumIngressEgressFailedToBuildAllBatchCallEvent = defineEvent(
  'EthereumIngressEgress.FailedToBuildAllBatchCall',
  ethereumIngressEgressFailedToBuildAllBatchCall,
);
