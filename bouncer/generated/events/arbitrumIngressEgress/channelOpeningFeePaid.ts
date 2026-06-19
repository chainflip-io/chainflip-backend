import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressChannelOpeningFeePaid = z.object({ fee: numberOrHex });

export const arbitrumIngressEgressChannelOpeningFeePaidEvent = defineEvent(
  'ArbitrumIngressEgress.ChannelOpeningFeePaid',
  arbitrumIngressEgressChannelOpeningFeePaid,
);
