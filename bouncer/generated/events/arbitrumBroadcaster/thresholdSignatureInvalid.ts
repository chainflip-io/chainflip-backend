import { z } from 'zod';

export const arbitrumBroadcasterThresholdSignatureInvalid = z.object({ broadcastId: z.number() });
