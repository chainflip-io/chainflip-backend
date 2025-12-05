import { z } from 'zod';
import { numberOrHex } from '../common';

export const solanaIngressEgressChannelOpeningFeePaid = z.object({ fee: numberOrHex });
