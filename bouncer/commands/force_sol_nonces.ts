#!/usr/bin/env -S pnpm tsx

import { PublicKey } from '@solana/web3.js';
import { decodeSolAddress, runWithTimeoutAndExit } from '../shared/utils';
import { submitGovernanceExtrinsic } from '../shared/cf_governance';

async function forceRecoverSolNonce(nonceAddress: string, nonceValue: string) {
  await submitGovernanceExtrinsic(async (chainflip) =>
    chainflip.tx.environment.forceRecoverSolNonce(
      decodeSolAddress(new PublicKey(nonceAddress).toBase58()),
      decodeSolAddress(new PublicKey(nonceValue).toBase58()),
    ),
  );
}

async function main() {
  await forceRecoverSolNonce(
    '2cNMwUCF51djw2xAiiU54wz1WrU8uG4Q8Kp8nfEuwghw',
    '8TZPeAWABEaH56pCBarps51BBfDXZxwTLXrGrdnVHMuU',
  );
  await forceRecoverSolNonce(
    'HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo',
    '9a2YpiECfkCiYPVkvBTmJv1K1e6sgQetWg8GeiJeNg2',
  );
  await forceRecoverSolNonce(
    'HDYArziNzyuNMrK89igisLrXFe78ti8cvkcxfx4qdU2p',
    'ATYZxNFThQDSo5yodfCDvh6YZtFPBZqxyvikZupQtD5v',
  );
  await forceRecoverSolNonce(
    'HLPsNyxBqfq2tLE31v6RiViLp2dTXtJRgHgsWgNDRPs2',
    'AM9PB2yZS22EShV4CQbu9mEDsWyqM44HPwQvmHUhmgyU',
  );
  await forceRecoverSolNonce(
    'GKMP63TqzbueWTrFYjRwMNkAyTHpQ54notRbAbMDmePM',
    '9FmKhHZGd1nMtYrCmvWFkXntSwGdxZAaGU3BGyAGMPxU',
  );
  await forceRecoverSolNonce(
    'EpmHm2aSPsB5ZZcDjqDhQ86h1BV32GFCbGSMuC58Y2tn',
    'AyqJtbdjRT35CmJKfaiCjL6DCe5LfRjcNRkCtNhthmgg',
  );
  await forceRecoverSolNonce(
    '9yBZNMrLrtspj4M7bEf2X6tqbqHxD2vNETw8qSdvJHMa',
    'D5ftBgbW2DQVbdRDXnU86fyF75EYjXahcFZhz2J6bQ3M',
  );
  await forceRecoverSolNonce(
    'J9dT7asYJFGS68NdgDCYjzU2Wi8uBoBusSHN1Z6JLWna',
    '3zxwonz8r5zY8c2TaFzmQfD2NTWGe8sShF4cA5eyBxLb',
  );
  await forceRecoverSolNonce(
    'GUMpVpQFNYJvSbyTtUarZVL7UDUgErKzDTSVJhekUX55',
    'H9iuxcwHporFNG9Ufx6GbMJzxKshGTXVpFYRN3BxGXhL',
  );
  await forceRecoverSolNonce(
    'AUiHYbzH7qLZSkb3u7nAqtvqC7e41sEzgWjBEvXrpfGv',
    'GoA9YN3X4dnNruw8KS8pykoziopE2iThyXypczD1JnxB',
  );
  await forceRecoverSolNonce(
    'BN2vyodNYQQTrx3gtaDAL2UGGVtZwFeF5M8krE5aYYES',
    '6M2xhGzhWQWSm1hn6SsZrSxXqwedJcvVCUU1tgb8x5gF',
  );
  await forceRecoverSolNonce(
    'Gwq9TAQCjbJtdnmtxQa3PbHFfbr6YTUBMDjEP9x2uXnH',
    '5hguE1bPSHGiHfzc3iCEL3TR1Uodk692sTDduRfb5y73',
  );
  await forceRecoverSolNonce(
    '3pGbKatko2ckoLEy139McfKiirNgy9brYxieNqFGdN1W',
    'hpFN3hwRjhx3Jdo3jTWtizbt8DvVBHBUTwKbqrxEDTU',
  );
  await forceRecoverSolNonce(
    '9Mcd8BTievK2yTvyiqG9Ft4HfDFf6mjGFBWMnCSRQP8S',
    'FfsUaqctjptQfy2LpXEprW9fHpAFpMJNDx8VxDkryto5',
  );
  await forceRecoverSolNonce(
    'AEZG74RoqM6sxf79eTizq5ShB4JTuCkMVwUgtnC8H94z',
    '6YJDJebBCgdXo6jTnHqztAxrrcVP7ARmDU532C3rvJQ3',
  );
  await forceRecoverSolNonce(
    'APLkgyCWi8DFAMF4KikjTu8YnUG1r7sMjVEfDiaBRZnS',
    'D25suHJDnB8kzBbT1zTVPqNzbSPB44ueVfGE5Fpcf7bm',
  );
  await forceRecoverSolNonce(
    '4ShNXTTHvpVt6bQdZTRdyW6yWXDzrPupdMuxajbEoGE4',
    '5SY1EquN355fvmr1Us9QF5NkgFtnpbemGRnWugpVJ1S6',
  );
  await forceRecoverSolNonce(
    'FgZp6NJYWw15U51ynfXCfU9vq3eVgDDAHMSfJ8fFBZZ8',
    'G1NKPL2YtdxCPS645sYqiRw2o93GhtXJhVsJM4WTpYPL',
  );
  await forceRecoverSolNonce(
    'ENQ9Mmg87KFLX8ncXRPDBSd7jhKCtPBi8QzAh4rkREgP',
    'EoXakeH1rPj9aU4DUuGL3AiJNkXi54H13AJVug6Fp8a4',
  );
  await forceRecoverSolNonce(
    'Hhay1UwkzkFUgrGUYuiCvUwv7kErNzAcZnVRQ2fetT7K',
    'FjsyfRZvnqssMGihxi1JDhWf8ykhyGqXp1Z38R4iR4KH',
  );
  await forceRecoverSolNonce(
    '2fUVR42opcHgGLrY1eguDXLYfQPHQe9ReJNmRorVt9v8',
    'Eqq9jnfVT3RfGZoq1VaxDwb7j3fnL7eBVunydveiEZ1t',
  );
  await forceRecoverSolNonce(
    'HfKr1wJASkW5UHs8yNWAqMeaYJdp8K2mdYwkbdVRdVrm',
    '8SmRtgVHdd6n5kqGPoo8Lj5eoBU7XrUm9ttq7HcdAeXj',
  );
  await forceRecoverSolNonce(
    'DrpYkMpJWkpNqX9yYgQfc3uZrCVYobJ3RbTABcSkHJkM',
    'EEMocPQ355GrihDWApEak8f4Bo5BWiBrWxJw16Lh51Gc',
  );
  await forceRecoverSolNonce(
    'HCXc3o2go1Y2KhfnykLYXEvofLifXTb7GT13w4GsFmGw',
    '2JH9XGSHzjpFucS1K1tVDu3ScccGDJke19Fog8umT1cB',
  );
  await forceRecoverSolNonce(
    'FFKYhae4HSnMmA6JJfe8NNtZeySA9yRWLaHzE2jqfhBr',
    'JCFTCXvAMoLLunMSKNfEtHb83Mc5H2Ruorq8kwKU2vS',
  );
  await forceRecoverSolNonce(
    'AaRrJovR9Npna4fuCJ17AB3cJAMzoNDaZymRTbGGzUZm',
    '7fPQo83ULM4g5wfGABCL2HjxMTXoAcvY7WnqQMzPU8ey',
  );
  await forceRecoverSolNonce(
    '5S8DzBBLvJUeyJccV4DekAK8KJA5PDcjwxRxCvgdyBEi',
    '6Mf4dBPMs5T8dHBzhyiQvXRYjB4HfdTBNGzWbNKihvkG',
  );
  await forceRecoverSolNonce(
    'Cot1DQZpm859brrre7swrDhTYLj2NJbg3hdMKCHk5zSk',
    'ugpSqqpQcXYybLeUumrKiDz7MJsbwvXudFFgBhVBYj8',
  );
  await forceRecoverSolNonce(
    '4mfDv7PisvtMhiyGmvD6vxRdVpB842XbUhimAZYxMEn9',
    '8MQVzBr3RvdCQ5UPrGfMyfQbPkcjrmC2TRxxknUpp17q',
  );
  await forceRecoverSolNonce(
    'BHW7qFCNHTX5QD5yJpT1hn1VM817Ji5ksZqiXMfqGrsj',
    'ihRRebrqvfSQ179gEsHzcfK8LseQNjG8qxWMzoWhPz1',
  );
  await forceRecoverSolNonce(
    'EJqZLeaxi2gVsJgQW4nbmxyWJukK25n7jB8qWKoDgWUN',
    '2hW1amHnHuRPGkhTK65sQE9ccz269XL1N6n3XgRay7BH',
  );
  await forceRecoverSolNonce(
    'BJqTPWyoqqgzhkLh1pbPh4KWBqg8kCUNzJ81avitSQrm',
    'Gd7uCmncEREuVFp2fXAQts1YDF7xNy5g7dkzTNLHKidT',
  );
  await forceRecoverSolNonce(
    'EkmPmEmSbwm8EDDYtLtaDgcfuLNtW7MbKx5w3FUpaGjv',
    '2nnJCWKtycWTs3VnNUD1BzxYJKHpMwTfuBMv3YCQKKoX',
  );
  await forceRecoverSolNonce(
    'CgwtCv8HQ67imnHEkz24TfXfyA2H5jurxcLGxAgDmNQj',
    '6y6Ja45rAWgZEdovZhKkNWxVJJ7AaQdQhM6RfPhyESqU',
  );
  await forceRecoverSolNonce(
    'zfKsXSxJ4cTpKS7S6aHL1Hy3m1CEjQuySKSwkWvukQX',
    '635q9xuqpThW4UXp7ABvTzoZfeawkHPR7tNTtGtfjdzW',
  );
  await forceRecoverSolNonce(
    '2VvN1s6txNYyBdKpaC8b6AZKVqUQiQT2Exrpa7ffCgV6',
    'ABZV1PZnrJSFMe9C8C3p9C1emgL8PVLbUXGp6wM4aGFW',
  );
  await forceRecoverSolNonce(
    'A2DT1dc4rA1uMry7WCLwoUEQQNjCAsAMkB4X9Lgo88zd',
    'GPKU2dGqWyLvHKwHWrWmscH5TEjhNcxdwwrMTPx6tLM8',
  );
  await forceRecoverSolNonce(
    '9mNBRGfTMLsSsQUn4YZfRDBVXfQ6juEWbNUTwv2ir9gC',
    '7PBYy3BaEtnP1H4j8toynDv7iJD7X8pW6tCEkBuZkfV7',
  );
  await forceRecoverSolNonce(
    '3jXiydxPx1P7Ggdja5yt384ryLJAW2c8LRGV8PPRT54C',
    'C6pejmGSjJerpJUbV3zaHYKDXDkPyAF9SJcYwsHCQ7og',
  );
  await forceRecoverSolNonce(
    '7ztGR1z28NpYjUaXyrGBzBGu62u1f9H9Pj9UVSKnT3yu',
    'FERfFzvtiQZ88x4nLyPp74XUmd45N5rneaLXrp95xWUi',
  );
  await forceRecoverSolNonce(
    '4GdnDTr5X4eJFHuzTEBLrz3tsREo8rQro7S9YDqrbMZ9',
    '2sAvRoDqKc7fwqPUReBDJFnZABHvTZS52No3prT8QBV4',
  );
  await forceRecoverSolNonce(
    'ALxnH6TBKJPBFRfFZspQkxDjb9nGLUP5oxFFdZNRFgUu',
    '4ftNy1xY73CE5Tvt2k6wP7FrkLYaKFdbqTxcTr1nHJoB',
  );
  await forceRecoverSolNonce(
    'Bu3sdWtBh5TJishgK3vneh2zJg1rjLqWN5mFTHxWspwJ',
    'CDH52v1QQFWDc2iGd44c6mR2GsRaxW7j3mU5a8iPmZxj',
  );
  await forceRecoverSolNonce(
    'GvBbUTE312RXU5iXAcNWt6CuVbfsPs5Nk28D6qvU6NF3',
    '9jEjbmo9PY1VXN5n8ynvsHTxgVgqkkyX3v37gtWLwdVL',
  );
  await forceRecoverSolNonce(
    '2LLct8SsnkW3sD9Gu8CfxmDEjKAWtFXqLvA8ymMyuq8u',
    'HPzoaZmsaiCLQh8hYcq8kAEb1n7RRmBiwznMgnHuiev9',
  );
  await forceRecoverSolNonce(
    'CQ9vUhC3dSa4LyZCpWVpNbXhSn6f7J3NQXWDDvMMk6aW',
    'DJ4ZLxfa3Rzueo9zqY2zXJRkaoJwvNRigxemAz1z5Pxm',
  );
  await forceRecoverSolNonce(
    'Cw8GqRmKzCbp7UFfafECC9sf9f936Chgx3BkbSgnXfmU',
    '13V9qLXm93RJeTChrpzpLC2bLatGKYWRqfDtYnXroVVq',
  );
  await forceRecoverSolNonce(
    'GFJ6m6YdNT1tUfAxyD2BiPSx8gwt3xe4jVAKdtdSUt8W',
    'EtNYHuALyNaWjLUSNRTppzkxc3eDntoQUwWrbfLBdvL',
  );
  await forceRecoverSolNonce(
    '7bphTuo5BKs4JJw5WPusCevmnoRk9ocFiB8EGgfwnh4c',
    'GvbiESUrxMH55onFzB3vg26JZ9KkGawNvy7Rdg2udgyE',
  );
  await forceRecoverSolNonce(
    'EFbUq18Mcdi2gGauRzmbNeD5ixaB7EYVk5JZgAF34LoS',
    '59gozJZEnmyzWKVY6HjodTfmddWyotpAWb2HiCq6xq3o',
  );
}

await runWithTimeoutAndExit(main(), 60);
