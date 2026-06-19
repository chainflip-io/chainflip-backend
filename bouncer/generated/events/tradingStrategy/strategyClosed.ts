import { z } from 'zod';
import { accountId } from '../common';
import { defineEvent } from '@chainflip/processor/event';

export const tradingStrategyStrategyClosed = z.object({ strategyId: accountId });

export const tradingStrategyStrategyClosedEvent = defineEvent(
  'TradingStrategy.StrategyClosed',
  tradingStrategyStrategyClosed,
);
