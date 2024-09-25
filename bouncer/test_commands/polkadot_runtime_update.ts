#!/usr/bin/env -S pnpm tsx
import { testPolkadotRuntimeUpdate } from '../tests/polkadot_runtime_update';

// Note: This test only passes if there is more than one node in the network due to the polkadot runtime upgrade causing broadcast failures due to bad signatures.
await testPolkadotRuntimeUpdate.runAndExit();
