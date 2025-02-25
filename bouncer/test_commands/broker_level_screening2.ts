#!/usr/bin/env -S pnpm tsx
import { testBrokerLevelScreening } from '../tests/broker_level_screening2';

await testBrokerLevelScreening();