import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressChannelOpeningFeePaid = z.object({ fee: numberOrHex });

export const polkadotIngressEgressChannelOpeningFeePaidEvent = defineEvent(
  'PolkadotIngressEgress.ChannelOpeningFeePaid',
  polkadotIngressEgressChannelOpeningFeePaid,
);
