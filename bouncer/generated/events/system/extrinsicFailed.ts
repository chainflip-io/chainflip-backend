import { z } from 'zod';
import { frameSystemDispatchEventInfo, spRuntimeDispatchError } from '../common';

export const systemExtrinsicFailed = z.object({
  dispatchError: spRuntimeDispatchError,
  dispatchInfo: frameSystemDispatchEventInfo,
});
