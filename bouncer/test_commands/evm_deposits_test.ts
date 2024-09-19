#!/usr/bin/env -S pnpm tsx
import { testEvmDeposits } from '../tests/evm_deposits';

await testEvmDeposits.runAndExit();
