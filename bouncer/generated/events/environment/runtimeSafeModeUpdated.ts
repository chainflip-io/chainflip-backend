import { z } from 'zod';
import { palletCfEnvironmentSafeModeUpdate } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const environmentRuntimeSafeModeUpdated = z.object({
  safeMode: palletCfEnvironmentSafeModeUpdate,
});

export const environmentRuntimeSafeModeUpdatedEvent = defineEvent(
  'Environment.RuntimeSafeModeUpdated',
  environmentRuntimeSafeModeUpdated,
);
