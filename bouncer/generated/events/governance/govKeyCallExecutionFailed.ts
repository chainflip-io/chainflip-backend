import { z } from 'zod';
import { hexString, spRuntimeDispatchError } from '../common';

export const governanceGovKeyCallExecutionFailed = z.object({
  callHash: hexString,
  error: spRuntimeDispatchError,
});
