import { z } from 'zod';

export const assethubBroadcasterThresholdSignatureInvalid = z.object({ broadcastId: z.number() });
