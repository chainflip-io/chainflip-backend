import { z } from 'zod';
import { frameSystemDispatchEventInfo, spRuntimeDispatchError } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const systemExtrinsicFailed = z.object({
  dispatchError: spRuntimeDispatchError,
  dispatchInfo: frameSystemDispatchEventInfo,
});

export const systemExtrinsicFailedEvent = defineEvent(
  'System.ExtrinsicFailed',
  systemExtrinsicFailed,
);
