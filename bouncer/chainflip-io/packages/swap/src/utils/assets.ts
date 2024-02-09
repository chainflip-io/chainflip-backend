import { Asset } from '@/shared/enums';
import { chainflipAsset } from '@/shared/parsers';

export const isSupportedAsset = (value: string): value is Asset =>
  chainflipAsset.safeParse(value).success;

export function assertSupportedAsset(value: string): asserts value is Asset {
  if (!isSupportedAsset(value)) {
    throw new Error(`received invalid asset "${value}"`);
  }
}
