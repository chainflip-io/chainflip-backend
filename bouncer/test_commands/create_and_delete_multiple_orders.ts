#!/usr/bin/env -S pnpm tsx
import { testCancelOrdersBatch } from '../tests/create_and_delete_multiple_orders';

await testCancelOrdersBatch.run();
