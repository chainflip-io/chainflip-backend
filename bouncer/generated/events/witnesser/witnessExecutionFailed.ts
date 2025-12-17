import { z } from 'zod';
import { hexString, spRuntimeDispatchError } from '../common';

export const witnesserWitnessExecutionFailed = z.object({
  callHash: hexString,
  error: spRuntimeDispatchError,
});
