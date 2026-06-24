import { z } from 'zod';
import { spRuntimeDispatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const emissionsFlipBurnSkipped = z.object({ reason: spRuntimeDispatchError });

export const emissionsFlipBurnSkippedEvent = defineEvent(
  'Emissions.FlipBurnSkipped',
  emissionsFlipBurnSkipped,
);
