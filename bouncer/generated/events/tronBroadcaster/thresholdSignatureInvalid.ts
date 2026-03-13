import { z } from 'zod';

export const tronBroadcasterThresholdSignatureInvalid = z.object({ broadcastId: z.number() });
