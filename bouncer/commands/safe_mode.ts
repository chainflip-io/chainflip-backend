// INSTRUCTIONS
//
// This command takes one or more arguments.
// It will set the SafeMode state of the chain to the value provided by the first argument.
// Valid arguments are "green", "amber" and "red".
// For example: pnpm tsx ./commands/safe_mode.ts green
// For "amber" mode, you can specify which features should remain enabled. For this, as the second argument
// provide a comma separated list (without spaces!) of the features that should remain enabled.
// Available features are:
// emissions_emissionsSyncEnabled, funding_redeemEnabled, swapping_swapsEnabled, swapping_withdrawalsEnabled,
// liquidityProvider_depositEnabled, liquidityProvider_withdrawalEnabled, validator_authorityRotationEnabled
// For example: pnpm tsx ./commands/safe_mode.ts amber swapping_swapsEnabled,swapping_withdrawalsEnabled

import { runWithTimeout } from '../shared/utils';
import { setSafeModeToGreen, setSafeModeToAmber, setSafeModeToRed } from '../shared/safe_mode';

async function main() {
  const mode = process.argv[2].toUpperCase();
  switch (mode) {
    case 'GREEN': {
      await setSafeModeToGreen();
      break;
    }
    case 'RED': {
      await setSafeModeToRed();
      break;
    }
    case 'AMBER': {
      const options: string[] = process.argv[3] ? process.argv[3].split(',') : [];
      await setSafeModeToAmber(options);
      break;
    }
    default: {
      console.log('Invalid safe mode. Valid values are RED AMBER and GREEN.');
      process.exit(1);
    }
  }
  process.exit(0);
}

runWithTimeout(main(), 120000).catch((error) => {
  console.error(error);
  process.exit(-1);
});
