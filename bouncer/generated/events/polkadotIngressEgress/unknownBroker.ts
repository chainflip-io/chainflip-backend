import { z } from 'zod';
import { accountId } from '../common';

export const polkadotIngressEgressUnknownBroker = z.object({ brokerId: accountId });
