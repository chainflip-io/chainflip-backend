import { z } from 'zod';
import { accountId } from '../common';

export const bitcoinIngressEgressUnknownBroker = z.object({ brokerId: accountId });
