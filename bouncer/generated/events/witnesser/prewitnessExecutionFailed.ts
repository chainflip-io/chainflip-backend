import { z } from 'zod';
import { hexString, spRuntimeDispatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const witnesserPrewitnessExecutionFailed = z.object({
  callHash: hexString,
  error: spRuntimeDispatchError,
});

export const witnesserPrewitnessExecutionFailedEvent = defineEvent(
  'Witnesser.PrewitnessExecutionFailed',
  witnesserPrewitnessExecutionFailed,
);
