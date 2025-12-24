import { z } from 'zod';

export const ethereumBroadcasterThresholdSignatureInvalid = z.object({ broadcastId: z.number() });
