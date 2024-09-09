#!/usr/bin/env -S pnpm tsx
import { testBrokerFeeCollection } from '../tests/broker_fee_collection';

await testBrokerFeeCollection.runAndExit();
