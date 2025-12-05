import { z } from 'zod';

export const solanaBroadcasterThresholdSignatureInvalid = z.object({ broadcastId: z.number() });
