import { z } from 'zod';
import { frameSupportDispatchDispatchInfo } from '../common';

export const systemExtrinsicSuccess = z.object({ dispatchInfo: frameSupportDispatchDispatchInfo });
