import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const ethereumIngressEgressUnknownBroker = z.object({ brokerId: accountId });

export const ethereumIngressEgressUnknownBrokerEvent = defineEvent(
  'EthereumIngressEgress.UnknownBroker',
  ethereumIngressEgressUnknownBroker,
);
