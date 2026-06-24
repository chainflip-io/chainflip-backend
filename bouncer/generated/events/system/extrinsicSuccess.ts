import { z } from 'zod';
import { frameSystemDispatchEventInfo } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const systemExtrinsicSuccess = z.object({ dispatchInfo: frameSystemDispatchEventInfo });

export const systemExtrinsicSuccessEvent = defineEvent(
  'System.ExtrinsicSuccess',
  systemExtrinsicSuccess,
);
