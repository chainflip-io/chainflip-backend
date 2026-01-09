import { z } from 'zod';
import { numberOrHex } from '../common';

export const assethubIngressEgressChannelOpeningFeePaid = z.object({ fee: numberOrHex });
