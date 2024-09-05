#!/usr/bin/env -S pnpm tsx
import { testBrokerFeeCollection } from '../shared/broker_fee_collection';

await testBrokerFeeCollection.execute();
