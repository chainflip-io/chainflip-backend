import { z } from 'zod';
import { hexString, spRuntimeDispatchError } from '../common';

export const witnesserPrewitnessExecutionFailed = z.object({
  callHash: hexString,
  error: spRuntimeDispatchError,
});
