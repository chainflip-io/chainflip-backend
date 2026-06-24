import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const polkadotIngressEgressUnknownBroker = z.object({ brokerId: accountId });

export const polkadotIngressEgressUnknownBrokerEvent = defineEvent(
  'PolkadotIngressEgress.UnknownBroker',
  polkadotIngressEgressUnknownBroker,
);
