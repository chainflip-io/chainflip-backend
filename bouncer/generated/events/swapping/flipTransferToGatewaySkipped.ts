import { z } from 'zod';
import { spRuntimeDispatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const swappingFlipTransferToGatewaySkipped = z.object({ reason: spRuntimeDispatchError });

export const swappingFlipTransferToGatewaySkippedEvent = defineEvent(
  'Swapping.FlipTransferToGatewaySkipped',
  swappingFlipTransferToGatewaySkipped,
);
