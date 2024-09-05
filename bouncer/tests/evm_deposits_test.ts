#!/usr/bin/env -S pnpm tsx
import { testEvmDeposits } from '../shared/evm_deposits';

await testEvmDeposits.execute();
