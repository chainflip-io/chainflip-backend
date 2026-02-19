import { z } from 'zod';
import { frameSystemDispatchEventInfo } from '../common';

export const systemExtrinsicSuccess = z.object({ dispatchInfo: frameSystemDispatchEventInfo });
