#!/usr/bin/env -S pnpm tsx
import { submitGovernanceExtrinsic } from 'shared/cf_governance';
import { runWithTimeoutAndExit } from 'shared/utils';

async function main() {
  await submitGovernanceExtrinsic((api) =>
    api.tx.lendingPools.updateWhitelist({
      SetAllowAll: true,
      // SetAllowedAccounts: [
      //   'cFKKYDgKLHgKRfHEwTPsGj2SJmmha5mGqajHEPXo1Chaqa96Q',
      //   'cFMN6cdEBAVThLGBhdYiZjLzBU6GkmNdNuvi2uXE8qE2KWPGK',
      //   'cFJhEEfJVueVnjYppmYuTZdje3GMQaBeAgFXpVAfTv1ZSdoyu',
      //   'cFNzx4s253hH7U9vtzVPAWAnpw2wdHJe8qkokYMCNwJcDRye1',
      //   'cFNzqMYdNi6RmvyjHViZfdmtw1TgrqrfxEZZ3vBDrMLa9FCJA',
      //   'cFPPUgTVZfCGupS8gMHqaUJqLbVS1w7GgDFWRJpe95hkzCFwk',
      //   'cFHsUq1uK5opJudRDd1GdUiX66wFk4pSqMZhzYJVgJqX5f9uJ',
      //   'cFM67FfxwRn36DCLzdPa1FNW9agXbMidgH2xnbDVo4u8U6RCi',
      //   'cFPdef3hF5zEwbWUG6ZaCJ3X7mTvEeAog7HxZ8QyFcCgDVGDM',
      // ],
    }),
  );
}
await runWithTimeoutAndExit(main(), 60);
