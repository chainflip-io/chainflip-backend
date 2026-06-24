import { z } from 'zod';
import { numberOrHex } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressChannelOpeningFeePaid = z.object({ fee: numberOrHex });

export const ethereumIngressEgressChannelOpeningFeePaidEvent = defineEvent(
  'EthereumIngressEgress.ChannelOpeningFeePaid',
  ethereumIngressEgressChannelOpeningFeePaid,
);
