#!/usr/bin/env -S pnpm tsx
import { executeWithTimeout } from '../shared/utils';
import { testBrokerFeeCollection } from '../shared/broker_fee_collection';

await executeWithTimeout(testBrokerFeeCollection(), 200);
