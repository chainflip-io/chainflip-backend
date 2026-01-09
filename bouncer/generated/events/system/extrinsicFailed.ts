import { z } from 'zod';
import { frameSupportDispatchDispatchInfo, spRuntimeDispatchError } from '../common';

export const systemExtrinsicFailed = z.object({
  dispatchError: spRuntimeDispatchError,
  dispatchInfo: frameSupportDispatchDispatchInfo,
});
