import { getChainflipApi, observeEvent } from './utils';
import { submitGovernanceExtrinsic } from './cf_governance';

async function setSafeMode(mode: string, options?: TranslatedOptions) {
  const chainflip = await getChainflipApi();

  const extrinsic = chainflip.tx.environment.updateSafeMode({ [mode]: options });
  const eventHandle = observeEvent('environment:RuntimeSafeModeUpdated', chainflip);
  await submitGovernanceExtrinsic(extrinsic);
  await eventHandle;
}

export async function setSafeModeToGreen() {
  await setSafeMode('CodeGreen');
}

export async function setSafeModeToRed() {
  await setSafeMode('CodeRed');
}

interface TranslatedOptions {
  [key: string]: { [key: string]: boolean };
}

export async function setSafeModeToAmber(options: string[]) {
  const translatedOptions: TranslatedOptions = {
    emissions: {},
    funding: {},
    swapping: {},
    liquidityProvider: {},
    validator: {},
    reputation: {},
    pools: {},
    vault: {},
  };
  options.forEach((x) => {
    try {
      const entry = x.split('_');
      translatedOptions[entry[0]][entry[1]] = true;
    } catch {
      console.log('The provided feature flag ' + x + ' is not supported!');
      process.exit(1);
    }
  });
  await setSafeMode('CodeAmber', translatedOptions);
}
