import { z } from 'zod';

export const bscBroadcasterThresholdSignatureInvalid = z.object({ broadcastId: z.number() });
