import { z } from 'zod';
import { numberOrHex } from '../common';

export const bitcoinIngressEgressChannelOpeningFeePaid = z.object({ fee: numberOrHex });
