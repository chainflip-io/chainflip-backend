import { z } from 'zod';
import { cfChainsAllBatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bscIngressEgressFailedToBuildAllBatchCall = z.object({ error: cfChainsAllBatchError });

export const bscIngressEgressFailedToBuildAllBatchCallEvent = defineEvent(
  'BscIngressEgress.FailedToBuildAllBatchCall',
  bscIngressEgressFailedToBuildAllBatchCall,
);
