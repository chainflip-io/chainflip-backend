import { z } from 'zod';
import { hexString, spRuntimeDispatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const witnesserWitnessExecutionFailed = z.object({
  callHash: hexString,
  error: spRuntimeDispatchError,
});

export const witnesserWitnessExecutionFailedEvent = defineEvent(
  'Witnesser.WitnessExecutionFailed',
  witnesserWitnessExecutionFailed,
);
