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
    'A6PxZEnTwTrLQG8pVBwytG8YLRqPUeEdsXHJP2UQ5RSF',
  );
  await forceRecoverSolNonce(
    'HVG21SovGzMBJDB9AQNuWb6XYq4dDZ6yUwCbRUuFnYDo',
    'EtK5bRt2pDX3CJyDzsdDtzjRf7v15NPR8JVpMikh6sNh',
  );
  await forceRecoverSolNonce(
    'HDYArziNzyuNMrK89igisLrXFe78ti8cvkcxfx4qdU2p',
    '3PkYapCizvyiFPBEg2pkiBAxnVYfpR2VWs7H6FQcD7rc',
  );
  await forceRecoverSolNonce(
    'HLPsNyxBqfq2tLE31v6RiViLp2dTXtJRgHgsWgNDRPs2',
    'HqPcXp31mYG1G4DP3c9F262pjXKeMzb4hbAwqsdLKrmM',
  );
  await forceRecoverSolNonce(
    'GKMP63TqzbueWTrFYjRwMNkAyTHpQ54notRbAbMDmePM',
    '2cMqnuCCnGWm56LFPe9mZuGHdhzpFpwwv2io9Q99EMjE',
  );
  await forceRecoverSolNonce(
    'EpmHm2aSPsB5ZZcDjqDhQ86h1BV32GFCbGSMuC58Y2tn',
    '7ZZpMge82HiNhhyv1LDzfwq7Ak9sF943TmLkQNuR7ZZh',
  );
  await forceRecoverSolNonce(
    '9yBZNMrLrtspj4M7bEf2X6tqbqHxD2vNETw8qSdvJHMa',
    'Ee2tKBQguV5Rfsa738jBTRCU7vczXkZYnddiqwSRz2Dz',
  );
  await forceRecoverSolNonce(
    'J9dT7asYJFGS68NdgDCYjzU2Wi8uBoBusSHN1Z6JLWna',
    'BhW9y8kkdBFiWnwzrYihhjhreovZd3TfZE7uaQnKz8ea',
  );
  await forceRecoverSolNonce(
    'GUMpVpQFNYJvSbyTtUarZVL7UDUgErKzDTSVJhekUX55',
    '5CGa6yRJsVStdMR4PkUNGW5F13UeHBuqyurkmNByrgxj',
  );
  await forceRecoverSolNonce(
    'AUiHYbzH7qLZSkb3u7nAqtvqC7e41sEzgWjBEvXrpfGv',
    'DCrChXBpKFjq61yYdULyYEnfqtcYkf1ACQqDNkgfwhF9',
  );
  await forceRecoverSolNonce(
    'BN2vyodNYQQTrx3gtaDAL2UGGVtZwFeF5M8krE5aYYES',
    '4fjG6oYKadvnsbzAzomF5k2Zdc4DuuUyT71nueAeykMW',
  );
  await forceRecoverSolNonce(
    'Gwq9TAQCjbJtdnmtxQa3PbHFfbr6YTUBMDjEP9x2uXnH',
    'GK29hbKjKWNwdF4KT11MzkrmQPsYPwE41qZMnLVcQPaS',
  );
  await forceRecoverSolNonce(
    '3pGbKatko2ckoLEy139McfKiirNgy9brYxieNqFGdN1W',
    '5cinXdpw2KAGzmiXXegWJRdDDboXbDHaQaT3WFsH3txb',
  );
  await forceRecoverSolNonce(
    '9Mcd8BTievK2yTvyiqG9Ft4HfDFf6mjGFBWMnCSRQP8S',
    'DRoAyPDtsg9CCMBSN6egFsWsP2zsQBAxCzN6fAdtQxJU',
  );
  await forceRecoverSolNonce(
    'AEZG74RoqM6sxf79eTizq5ShB4JTuCkMVwUgtnC8H94z',
    'G8ZKHMsWFSoKJAtVbm1xgv8VjT5F6YBeiZbbzpVHuuyM',
  );
  await forceRecoverSolNonce(
    'APLkgyCWi8DFAMF4KikjTu8YnUG1r7sMjVEfDiaBRZnS',
    'BMUqNXhMoB6VWsR7jHgRcv7yio8L5vjHdGby7gEJ4Pd2',
  );
  await forceRecoverSolNonce(
    '4ShNXTTHvpVt6bQdZTRdyW6yWXDzrPupdMuxajbEoGE4',
    '52yamKJdMiQ5tEUyhkngvjR3XFXp7dmJzYsVsLbPs9JX',
  );
  await forceRecoverSolNonce(
    'FgZp6NJYWw15U51ynfXCfU9vq3eVgDDAHMSfJ8fFBZZ8',
    'AX3qKNMBRKZimeCsBEhtp7heeduKekj85a4UpdN34HFe',
  );
  await forceRecoverSolNonce(
    'ENQ9Mmg87KFLX8ncXRPDBSd7jhKCtPBi8QzAh4rkREgP',
    'GGFme2ydkkbDzq7LhVDMX5SsFf2yGLf7uKNSLLhvrGMd',
  );
  await forceRecoverSolNonce(
    'Hhay1UwkzkFUgrGUYuiCvUwv7kErNzAcZnVRQ2fetT7K',
    'HMN14axscHALAuknuwSpVkEmAJkZWeJoNAjJuXUjRQbN',
  );
  await forceRecoverSolNonce(
    '2fUVR42opcHgGLrY1eguDXLYfQPHQe9ReJNmRorVt9v8',
    'RWoH6shzxCS9dmW2mg37cujXxARBRBunbHEtZwUz1Gj',
  );
  await forceRecoverSolNonce(
    'HfKr1wJASkW5UHs8yNWAqMeaYJdp8K2mdYwkbdVRdVrm',
    '2dhBBWYQE2Fvm4ShUQjno8ydJb5H6rUmBZ1e6TJHDupL',
  );
  await forceRecoverSolNonce(
    'DrpYkMpJWkpNqX9yYgQfc3uZrCVYobJ3RbTABcSkHJkM',
    '6VCThFyLtFCKk35wihyrRUa6dubBU7skhJdRRDBfH4Md',
  );
  await forceRecoverSolNonce(
    'HCXc3o2go1Y2KhfnykLYXEvofLifXTb7GT13w4GsFmGw',
    'EggsCqUiKJVxmN7a9s7RScXUAJRjekjwCAaeQvK9TcJ4',
  );
  await forceRecoverSolNonce(
    'FFKYhae4HSnMmA6JJfe8NNtZeySA9yRWLaHzE2jqfhBr',
    '2E8BayMrqL3on6bhcMms6dsm3PwKcxttFSuHDNe6vZ4B',
  );
  await forceRecoverSolNonce(
    'AaRrJovR9Npna4fuCJ17AB3cJAMzoNDaZymRTbGGzUZm',
    'D5bhT4vxhrtkfPeyZbvCtwzAnHSwBaa287CZZU8F6fye',
  );
  await forceRecoverSolNonce(
    '5S8DzBBLvJUeyJccV4DekAK8KJA5PDcjwxRxCvgdyBEi',
    '8o7RkbTeV9r1yMgXaTzjcPys2FEkqieHER6Z5Fdc8hbw',
  );
  await forceRecoverSolNonce(
    'Cot1DQZpm859brrre7swrDhTYLj2NJbg3hdMKCHk5zSk',
    'Gd86jHRKKSxrho3WKni5HYe6runTFX4cyFUQtcmJeiuk',
  );
  await forceRecoverSolNonce(
    '4mfDv7PisvtMhiyGmvD6vxRdVpB842XbUhimAZYxMEn9',
    '4YQLN6N7G9nFwT8UVdFE2ZniW1gf89Qjob16wxMThxqN',
  );
  await forceRecoverSolNonce(
    'BHW7qFCNHTX5QD5yJpT1hn1VM817Ji5ksZqiXMfqGrsj',
    'Ft3vnX4KQBa22CVpPkmvk5QNMGwL2xhQVxQtFJwK5MvJ',
  );
  await forceRecoverSolNonce(
    'EJqZLeaxi2gVsJgQW4nbmxyWJukK25n7jB8qWKoDgWUN',
    '5tZeUYorHfdh9FYsA9yKjanFRwxjGxM9YLzNAfiwhZUf',
  );
  await forceRecoverSolNonce(
    'BJqTPWyoqqgzhkLh1pbPh4KWBqg8kCUNzJ81avitSQrm',
    '5ggDcExaPfgjsmrhBS3D2UnRaEPsCGKGDkJqzF7gr92A',
  );
  await forceRecoverSolNonce(
    'EkmPmEmSbwm8EDDYtLtaDgcfuLNtW7MbKx5w3FUpaGjv',
    '3G7D13ckfLCfDFC3VusXdittLHp6z6dZeUJBHTqcc2iR',
  );
  await forceRecoverSolNonce(
    'CgwtCv8HQ67imnHEkz24TfXfyA2H5jurxcLGxAgDmNQj',
    'Gikpdfo6SwRqi3nTmQKuCmap3cGwupZd2poiYkUop4Sn',
  );
  await forceRecoverSolNonce(
    'zfKsXSxJ4cTpKS7S6aHL1Hy3m1CEjQuySKSwkWvukQX',
    '43Kn8Kevfy2cy2fpJEhEZSpmPNwFurL2ERG5FqdSeTsq',
  );
  await forceRecoverSolNonce(
    '2VvN1s6txNYyBdKpaC8b6AZKVqUQiQT2Exrpa7ffCgV6',
    'FVZKFoZ8WRdsFBp64LpTF1MrH36dHym2XZv7cJ2oYU5',
  );
  await forceRecoverSolNonce(
    'A2DT1dc4rA1uMry7WCLwoUEQQNjCAsAMkB4X9Lgo88zd',
    'HjtHG8XRKyGiN8VXWmMh9oEviURAv5o2VygKTvsZjAPz',
  );
  await forceRecoverSolNonce(
    '9mNBRGfTMLsSsQUn4YZfRDBVXfQ6juEWbNUTwv2ir9gC',
    '2S1aHds5wqUSyY4BmAK9YyBNGwzsQSsqGa2iyisF8t6h',
  );
  await forceRecoverSolNonce(
    '3jXiydxPx1P7Ggdja5yt384ryLJAW2c8LRGV8PPRT54C',
    'Hgu6wkD6aE3uuESdW9fCWoXX4sN3eLnxYJsM7QCtrZMk',
  );
  await forceRecoverSolNonce(
    '7ztGR1z28NpYjUaXyrGBzBGu62u1f9H9Pj9UVSKnT3yu',
    '9wic99ejEMQubHea9KKZZk27EU7r4LL8672D5GNrpXRG',
  );
  await forceRecoverSolNonce(
    '4GdnDTr5X4eJFHuzTEBLrz3tsREo8rQro7S9YDqrbMZ9',
    'FCsgGf33ueodTVhLgQtTriNL5ZGfoaWoBkDwXSbDmLFd',
  );
  await forceRecoverSolNonce(
    'ALxnH6TBKJPBFRfFZspQkxDjb9nGLUP5oxFFdZNRFgUu',
    'QkBgGAKFPtQzRj1v7sFECr8D2LMPb99AEy3w1RtKX5j',
  );
  await forceRecoverSolNonce(
    'Bu3sdWtBh5TJishgK3vneh2zJg1rjLqWN5mFTHxWspwJ',
    'GWvckQW231Safveswww1GBSu4SzP5h5SgE6gugBn8upC',
  );
  await forceRecoverSolNonce(
    'GvBbUTE312RXU5iXAcNWt6CuVbfsPs5Nk28D6qvU6NF3',
    'BnFsZdujQde7FnHGVcZTmvidRHBr5H87XRDDB6A5dn8D',
  );
  await forceRecoverSolNonce(
    '2LLct8SsnkW3sD9Gu8CfxmDEjKAWtFXqLvA8ymMyuq8u',
    'Bnt1CWn8SEwpNqFbNxby6ysoW49wmL95Ed28pbS9v4Nx',
  );
  await forceRecoverSolNonce(
    'CQ9vUhC3dSa4LyZCpWVpNbXhSn6f7J3NQXWDDvMMk6aW',
    '2yVSXvwXjtA5tqqWUKjxBuYjME6pKwJGA12NUc31x9VS',
  );
  await forceRecoverSolNonce(
    'Cw8GqRmKzCbp7UFfafECC9sf9f936Chgx3BkbSgnXfmU',
    'FDPW3e2qPvNrmi1dqxsMaYAXLq9vMQYda5fKsVzNBUCv',
  );
  await forceRecoverSolNonce(
    'GFJ6m6YdNT1tUfAxyD2BiPSx8gwt3xe4jVAKdtdSUt8W',
    '4tUTcUePrDUu48ZyH584aYv8JAbPrc9aDcH6bjxhEJon',
  );
  await forceRecoverSolNonce(
    '7bphTuo5BKs4JJw5WPusCevmnoRk9ocFiB8EGgfwnh4c',
    'SyhFE8iYH9ZsZNBDWLvTDTBFBoEjxs12msF3xprikgf',
  );
  await forceRecoverSolNonce(
    'EFbUq18Mcdi2gGauRzmbNeD5ixaB7EYVk5JZgAF34LoS',
    '53EBQYjZ7yH3Zy6KCWjSGAUGTki2YjxsHkrfXnvsj9vT',
  );
}

await runWithTimeoutAndExit(main(), 60);
