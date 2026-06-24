import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const bitcoinIngressEgressChannelOpeningFeePaid = z.object({ fee: numberOrHex });

export const bitcoinIngressEgressChannelOpeningFeePaidEvent = defineEvent(
  'BitcoinIngressEgress.ChannelOpeningFeePaid',
  bitcoinIngressEgressChannelOpeningFeePaid,
);
