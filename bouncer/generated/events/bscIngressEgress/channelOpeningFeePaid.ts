import { z } from 'zod';
import { numberOrHex } from '../common';

export const bscIngressEgressChannelOpeningFeePaid = z.object({ fee: numberOrHex });
