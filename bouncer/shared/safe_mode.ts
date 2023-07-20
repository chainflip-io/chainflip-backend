import Keyring from '@polkadot/keyring';
import { cryptoWaitReady } from '@polkadot/util-crypto';
import { getChainflipApi, handleSubstrateError, snowWhiteMutex } from './utils';

async function setSafeMode(mode: string, options?: TranslatedOptions) {
  const chainflip = await getChainflipApi();

  const snowWhiteUri =
    process.env.SNOWWHITE_URI ??
    'market outdoor rubber basic simple banana resist quarter lab random hurdle cruise';

  await cryptoWaitReady();

  const keyring = new Keyring({ type: 'sr25519' });

  const snowWhite = keyring.createFromUri(snowWhiteUri);

  const extrinsic = chainflip.tx.environment.updateSafeMode({ [mode]: options });
  return snowWhiteMutex.runExclusive(async () =>
    chainflip.tx.governance
      .proposeGovernanceExtrinsic(extrinsic)
      .signAndSend(snowWhite, { nonce: -1 }, handleSubstrateError(chainflip)),
  );
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
