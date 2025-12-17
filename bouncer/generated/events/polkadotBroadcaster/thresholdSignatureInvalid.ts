import { z } from 'zod';

export const polkadotBroadcasterThresholdSignatureInvalid = z.object({ broadcastId: z.number() });
