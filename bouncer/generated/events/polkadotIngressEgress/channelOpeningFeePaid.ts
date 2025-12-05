import { z } from 'zod';
import { numberOrHex } from '../common';

export const polkadotIngressEgressChannelOpeningFeePaid = z.object({ fee: numberOrHex });
