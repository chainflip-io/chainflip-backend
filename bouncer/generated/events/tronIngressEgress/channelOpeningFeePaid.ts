import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tronIngressEgressChannelOpeningFeePaid = z.object({ fee: numberOrHex });

export const tronIngressEgressChannelOpeningFeePaidEvent = defineEvent(
  'TronIngressEgress.ChannelOpeningFeePaid',
  tronIngressEgressChannelOpeningFeePaid,
);
