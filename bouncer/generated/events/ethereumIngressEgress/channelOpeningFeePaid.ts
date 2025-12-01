import { z } from 'zod';
import { numberOrHex } from '../common';

export const ethereumIngressEgressChannelOpeningFeePaid = z.object({ fee: numberOrHex });
