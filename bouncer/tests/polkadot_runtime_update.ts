#!/usr/bin/env -S pnpm tsx
import { testPolkadotRuntimeUpdate } from '../shared/polkadot_runtime_update';
import { executeWithTimeout } from '../shared/utils';

// Note: This test only passes if there is more than one node in the network due to the polkadot runtime upgrade causing broadcast failures due to bad signatures.
await executeWithTimeout(testPolkadotRuntimeUpdate(), 1300);
