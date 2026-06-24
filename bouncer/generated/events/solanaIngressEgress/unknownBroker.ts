import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const solanaIngressEgressUnknownBroker = z.object({ brokerId: accountId });

export const solanaIngressEgressUnknownBrokerEvent = defineEvent(
  'SolanaIngressEgress.UnknownBroker',
  solanaIngressEgressUnknownBroker,
);
