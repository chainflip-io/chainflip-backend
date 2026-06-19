import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressChannelOpeningFeePaid = z.object({ fee: numberOrHex });

export const solanaIngressEgressChannelOpeningFeePaidEvent = defineEvent(
  'SolanaIngressEgress.ChannelOpeningFeePaid',
  solanaIngressEgressChannelOpeningFeePaid,
);
