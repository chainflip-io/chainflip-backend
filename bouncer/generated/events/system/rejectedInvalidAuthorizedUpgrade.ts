import { z } from 'zod';
import { hexString, spRuntimeDispatchError } from '../common';

export const systemRejectedInvalidAuthorizedUpgrade = z.object({
  codeHash: hexString,
  error: spRuntimeDispatchError,
});
