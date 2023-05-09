#!/usr/bin/env node

import { BaseArgs, baseArgs } from "../../../src/input/baseArgs";
import { balanceView, equalityView } from "../views";
import { mapCmd } from "../../../src/commands/mapCmd";
import { reduceCmd } from "../../../src/commands/reduceCmd";
import { Show } from "../../../src/input/outputArgs";
import { isoDateOrTime } from "../../../src/time/timeUtils";

async function main(cliInput: string | string[]) {
  const args = (await baseArgs.parse(cliInput)) as BaseArgs;
  args.db ??= "finance";
  const allEqualityChecks = (
    await mapCmd({ ...args, mapName: equalityView.name, show: Show.None })
  ).rows;

  for (const row of allEqualityChecks) {
    const [account, currency, datetime] = row.key;
    const expectedBalance = row.value.toFixed(2);
    const actualBalance = (
      await reduceCmd({
        ...args,
        mapName: balanceView.name,
        start: `[${account}, ${currency}, "0"]`,
        end: `[${account}, ${currency}, "${datetime}"]`,
        show: Show.None,
      })
    ).rows[0].value.toFixed(2);
    if (expectedBalance !== actualBalance) {
      console.error(
        `Balance mismatch for ${account} ${currency} ${datetime}:\nexpected ${expectedBalance}, got ${actualBalance} (${
          actualBalance - expectedBalance
        })`
      );
      process.exit(3);
    }
    console.info(`Balance check passed for ${account} ${currency} ${datetime}`);
  }
}

type BalanceWatcherInput = {
  args: BaseArgs;
  account: string;
  currency: string;
  goodDate?: isoDateOrTime;
  failDate: isoDateOrTime;
};

async function balanceWatcher({
  args,
  account,
  currency,
  goodDate,
  failDate,
}: BalanceWatcherInput) {
  let initialGoodBalance;
  const goodBalance = goodDate
    ? (
        await reduceCmd({
          ...args,
          mapName: balanceView.name,
          start: `[${account}, ${currency}, "0"]`,
          end: `[${account}, ${currency}, ${goodDate}]`,
          show: Show.None,
        })
      ).rows[0].value.toFixed(2)
    : "0";
  const failBalance = (
    await reduceCmd({
      ...args,
      mapName: balanceView.name,
      start: `[${account}, ${currency}, "0"]`,
      end: `[${account}, ${currency}, "${failDate}"]`,
      show: Show.None,
    })
  ).rows[0].value.toFixed(2);
}

if (require.main === module) {
  main(process.argv.slice(2)).catch((err) => {
    console.error(err);
  });
}
