import { z } from 'zod';
import { numberOrHex } from '../common';

export const tronIngressEgressChannelOpeningFeePaid = z.object({ fee: numberOrHex });
