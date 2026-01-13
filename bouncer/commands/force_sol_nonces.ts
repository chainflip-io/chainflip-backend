#!/usr/bin/env -S pnpm tsx

import { PublicKey } from '@solana/web3.js';
import { decodeSolAddress, runWithTimeoutAndExit } from 'shared/utils';
import { submitGovernanceExtrinsic } from 'shared/cf_governance';

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
    'EBEB7LQogyDdABRDpZ7o2sDJdVveUJpgFJ7KurAbk8wq',
  );
  await forceRecoverSolNonce(
    'HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo',
    'DXbBqMj7gVAxQL9LYnEJJVcwxtYjQhcuH2LHCrvAYVJJ',
  );
  await forceRecoverSolNonce(
    'HDYArziNzyuNMrK89igisLrXFe78ti8cvkcxfx4qdU2p',
    'CN8wxZy4aKBA2fGYfukvho4PWQUbKGmk2ou1fEfJWL4e',
  );
  await forceRecoverSolNonce(
    'HLPsNyxBqfq2tLE31v6RiViLp2dTXtJRgHgsWgNDRPs2',
    'GtAJgcXXCVshXR6zRc95VnGMtoz8e4CNErTjfZfz4Bii',
  );
  await forceRecoverSolNonce(
    'GKMP63TqzbueWTrFYjRwMNkAyTHpQ54notRbAbMDmePM',
    'C2VqE1ggFBdkuff3ZNS33HHZALP3k7oeQgQRNrrG2f3h',
  );
  await forceRecoverSolNonce(
    'EpmHm2aSPsB5ZZcDjqDhQ86h1BV32GFCbGSMuC58Y2tn',
    'GGsKcpgwwtBFyLrFRciMoAoAx2d8bg9a8AUVCUMxaGub',
  );
  await forceRecoverSolNonce(
    '9yBZNMrLrtspj4M7bEf2X6tqbqHxD2vNETw8qSdvJHMa',
    'HNsnvQtmXetygwQX2EKELxrAqJ2F1XV8NXxa5ywgxBxG',
  );
  await forceRecoverSolNonce(
    'J9dT7asYJFGS68NdgDCYjzU2Wi8uBoBusSHN1Z6JLWna',
    '6iXPJUsyhHSKhfp8awdBqeBEQQfhRoqwhPkLbLr65jtW',
  );
  await forceRecoverSolNonce(
    'GUMpVpQFNYJvSbyTtUarZVL7UDUgErKzDTSVJhekUX55',
    '4riXqjWPSKoiwHtZpxj3R7urgjYJtB7VwVPaFMzRPp7V',
  );
  await forceRecoverSolNonce(
    'AUiHYbzH7qLZSkb3u7nAqtvqC7e41sEzgWjBEvXrpfGv',
    'C85rnatmcp4LMLXakJc8vc8Sjkv6fXwCv8yYg4ebnmyb',
  );
  await forceRecoverSolNonce(
    'BN2vyodNYQQTrx3gtaDAL2UGGVtZwFeF5M8krE5aYYES',
    'GpC6WpDYw4yR2K1VjQBk8ZrztxNzcQFq1YqVxCwdBs2c',
  );
  await forceRecoverSolNonce(
    'Gwq9TAQCjbJtdnmtxQa3PbHFfbr6YTUBMDjEP9x2uXnH',
    'DjYGg8PM95LMh27h3tjxXYRCBqzgMHmJEhXSGebP1M4w',
  );
  await forceRecoverSolNonce(
    '3pGbKatko2ckoLEy139McfKiirNgy9brYxieNqFGdN1W',
    '5grBNEdn9NCJaff1TjErfvoCpSHMviaFLm96ossoWDMi',
  );
  await forceRecoverSolNonce(
    '9Mcd8BTievK2yTvyiqG9Ft4HfDFf6mjGFBWMnCSRQP8S',
    '9Gwedc7prXQ8pnzKvrexW1ghFcAxJ2uRg95Egt3iqSHk',
  );
  await forceRecoverSolNonce(
    'AEZG74RoqM6sxf79eTizq5ShB4JTuCkMVwUgtnC8H94z',
    'CkmUkWZChbt6EA8tpcE168ievvPgzcjdwjNuPudRzjQm',
  );
  await forceRecoverSolNonce(
    'APLkgyCWi8DFAMF4KikjTu8YnUG1r7sMjVEfDiaBRZnS',
    'BgRgKsn8bYcaLhvaYc8Tv1oa4madD98NPRQWWcqtLjKL',
  );
  await forceRecoverSolNonce(
    '4ShNXTTHvpVt6bQdZTRdyW6yWXDzrPupdMuxajbEoGE4',
    'FU4YhV1MTa7DD3xKBAW5EYaZ1bVD58XoL8bVwewCpigZ',
  );
  await forceRecoverSolNonce(
    'FgZp6NJYWw15U51ynfXCfU9vq3eVgDDAHMSfJ8fFBZZ8',
    '6SVgYg7J8ot4Bkwn79XgMKX9fXjkgmKU1W5Hyo3qrxZD',
  );
  await forceRecoverSolNonce(
    'ENQ9Mmg87KFLX8ncXRPDBSd7jhKCtPBi8QzAh4rkREgP',
    'G3QUHkZxoVynkXy2D2gi6sVYtn6HceCSxg8Wh625akjc',
  );
  await forceRecoverSolNonce(
    'Hhay1UwkzkFUgrGUYuiCvUwv7kErNzAcZnVRQ2fetT7K',
    'F1HPqnoc3wD6wufL45ReB1jQYJ64c8gSYkzyrpRaXjaS',
  );
  await forceRecoverSolNonce(
    '2fUVR42opcHgGLrY1eguDXLYfQPHQe9ReJNmRorVt9v8',
    '9T3anXzYiNdfxQA3Y6U2AbVqP8dTjw4Hfs8WS9b4coCL',
  );
  await forceRecoverSolNonce(
    'HfKr1wJASkW5UHs8yNWAqMeaYJdp8K2mdYwkbdVRdVrm',
    'CJZ1SP6oUkMcETN2fhnxP3VCyoy71z7Ladr8vTSKr3XK',
  );
  await forceRecoverSolNonce(
    'DrpYkMpJWkpNqX9yYgQfc3uZrCVYobJ3RbTABcSkHJkM',
    'B7QwD1iEd8dstaeR1YWetZJfDCvDorGxJ6bZqWAkW9xf',
  );
  await forceRecoverSolNonce(
    'HCXc3o2go1Y2KhfnykLYXEvofLifXTb7GT13w4GsFmGw',
    'C4535AjYJYz9kcE98J6BGcj5PMm9MwY4NbvFgxPoregC',
  );
  await forceRecoverSolNonce(
    'FFKYhae4HSnMmA6JJfe8NNtZeySA9yRWLaHzE2jqfhBr',
    '8oeD6KYErSBhWu4SBVJ4Jny4Q9ym82tHQUsSEEbqTJj2',
  );
  await forceRecoverSolNonce(
    'AaRrJovR9Npna4fuCJ17AB3cJAMzoNDaZymRTbGGzUZm',
    '5zPBm2JqwRai8Fzk1Qvs517QAD5VpaV6gH35SsYrbq8u',
  );
  await forceRecoverSolNonce(
    '5S8DzBBLvJUeyJccV4DekAK8KJA5PDcjwxRxCvgdyBEi',
    '72wiNTamivY2M2wXgYvGiiTzgyj3UcEGqohDwCDHPuvH',
  );
  await forceRecoverSolNonce(
    'Cot1DQZpm859brrre7swrDhTYLj2NJbg3hdMKCHk5zSk',
    '5QTPu2coVwsir9vNhoizoe5mMxFsDiyrLqPdDeuUPitp',
  );
  await forceRecoverSolNonce(
    '4mfDv7PisvtMhiyGmvD6vxRdVpB842XbUhimAZYxMEn9',
    'CW4dmNjZscAyWk8uyVGrQpRB9AkTPxvoJrsT7uzVCU8m',
  );
  await forceRecoverSolNonce(
    'BHW7qFCNHTX5QD5yJpT1hn1VM817Ji5ksZqiXMfqGrsj',
    '9zE7sv8kLzNqtW1YS4rbB3ELoDBi6v7Q16EamwUKHM9Y',
  );
  await forceRecoverSolNonce(
    'EJqZLeaxi2gVsJgQW4nbmxyWJukK25n7jB8qWKoDgWUN',
    '2cbcX7M4Q9ixj9TXLZJZsMQJLVBW5W2ungid7J5L8x88',
  );
  await forceRecoverSolNonce(
    'BJqTPWyoqqgzhkLh1pbPh4KWBqg8kCUNzJ81avitSQrm',
    'Fs3Bhv8MiCz9sNBaGFnF5B7UHz1r5gnnaybiW3viiphU',
  );
  await forceRecoverSolNonce(
    'EkmPmEmSbwm8EDDYtLtaDgcfuLNtW7MbKx5w3FUpaGjv',
    'GRCBCmrxS2Cr2TUBArunJ268qRgN31D38z9ES6DKBYwd',
  );
  await forceRecoverSolNonce(
    'CgwtCv8HQ67imnHEkz24TfXfyA2H5jurxcLGxAgDmNQj',
    '7tXYrk9GoK9kHj1ZRDZoZaZoRJLPnjEhYRqJfpvL1u1R',
  );
  await forceRecoverSolNonce(
    'zfKsXSxJ4cTpKS7S6aHL1Hy3m1CEjQuySKSwkWvukQX',
    '4JWVRxcwLgByXu2738ZwA9q9jojnDYNQ3JBdbj9BAqHV',
  );
  await forceRecoverSolNonce(
    '2VvN1s6txNYyBdKpaC8b6AZKVqUQiQT2Exrpa7ffCgV6',
    '76RxLeAmcx9rBFJUZzRzn7PoEdPSWLEaKGj8FcZb7rac',
  );
  await forceRecoverSolNonce(
    'A2DT1dc4rA1uMry7WCLwoUEQQNjCAsAMkB4X9Lgo88zd',
    '3qdf8ynF25yHMgybPD7aGAwVrpJ3GujzakPQinReSvXD',
  );
  await forceRecoverSolNonce(
    '9mNBRGfTMLsSsQUn4YZfRDBVXfQ6juEWbNUTwv2ir9gC',
    '98v8VpmDPX13tznduNy3efbNzheRgMATJqwt7n9LcnRS',
  );
  await forceRecoverSolNonce(
    '3jXiydxPx1P7Ggdja5yt384ryLJAW2c8LRGV8PPRT54C',
    '6Fdr5hH5YyaZYUCn6uwXK3djyWoCZE733yWDTG1JoNDm',
  );
  await forceRecoverSolNonce(
    '7ztGR1z28NpYjUaXyrGBzBGu62u1f9H9Pj9UVSKnT3yu',
    'x6Q2hi57YkfyBqBvz5eF8Vd4ySdipqgS2A1841CPaaD',
  );
  await forceRecoverSolNonce(
    '4GdnDTr5X4eJFHuzTEBLrz3tsREo8rQro7S9YDqrbMZ9',
    '5czSBq1ZA48feqE1CJSJ5WkidYsj3GxCYiGuNfwWyggC',
  );
  await forceRecoverSolNonce(
    'ALxnH6TBKJPBFRfFZspQkxDjb9nGLUP5oxFFdZNRFgUu',
    'qaKvjDDsPQoKc7Pf8Q5TnjPo2nfCFwngS5U3AvEnebC',
  );
  await forceRecoverSolNonce(
    'Bu3sdWtBh5TJishgK3vneh2zJg1rjLqWN5mFTHxWspwJ',
    '3KBsr631nQSqXm1uH6BibU6daniUAEyaGmhQ1Uz5gSDq',
  );
  await forceRecoverSolNonce(
    'GvBbUTE312RXU5iXAcNWt6CuVbfsPs5Nk28D6qvU6NF3',
    'GUcygAGC7EjpG4cRThQCBCeEf1xonc3EsceEVHjXkNeW',
  );
  await forceRecoverSolNonce(
    '2LLct8SsnkW3sD9Gu8CfxmDEjKAWtFXqLvA8ymMyuq8u',
    'Bs89PWSKSon6irpF6PjZYfHGKTnza2EBm7ridouQ8b46',
  );
  await forceRecoverSolNonce(
    'CQ9vUhC3dSa4LyZCpWVpNbXhSn6f7J3NQXWDDvMMk6aW',
    '9UYty6imMQV7tMGaVLg6nLiJTWCmPXGuDAbwi3wuoyJG',
  );
  await forceRecoverSolNonce(
    'Cw8GqRmKzCbp7UFfafECC9sf9f936Chgx3BkbSgnXfmU',
    '2Qc4jT8DKAMd8TEF8rizbwPjwWSoEBENRcjtvWtVZHu7',
  );
  await forceRecoverSolNonce(
    'GFJ6m6YdNT1tUfAxyD2BiPSx8gwt3xe4jVAKdtdSUt8W',
    '3abTMJbmcQoDkwvXPWazMKrFkVqgppgNxyEo65QCqdKm',
  );
  await forceRecoverSolNonce(
    '7bphTuo5BKs4JJw5WPusCevmnoRk9ocFiB8EGgfwnh4c',
    '7YhvStjCqZKMLQNwjPz7QfriR1rASLoUWDrkKeBk76dJ',
  );
  await forceRecoverSolNonce(
    'EFbUq18Mcdi2gGauRzmbNeD5ixaB7EYVk5JZgAF34LoS',
    'HNUazUjm6GkAeT6x4hiZtBaFfetaNbXpSjzLXWoXB18T',
  );
}

await runWithTimeoutAndExit(main(), 60);
