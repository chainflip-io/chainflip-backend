import { z } from 'zod';
import { accountId } from '../common';

export const tradingStrategyStrategyClosed = z.object({ strategyId: accountId });
