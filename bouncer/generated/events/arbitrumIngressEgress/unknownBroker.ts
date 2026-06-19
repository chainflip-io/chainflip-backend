import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const arbitrumIngressEgressUnknownBroker = z.object({ brokerId: accountId });

export const arbitrumIngressEgressUnknownBrokerEvent = defineEvent(
  'ArbitrumIngressEgress.UnknownBroker',
  arbitrumIngressEgressUnknownBroker,
);
