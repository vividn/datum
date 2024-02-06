#!/usr/bin/env node

import { DbArgs, baseArgs } from "../../../src/input/baseArgs";
import { balanceView } from "../views";
import { HIGH_STRING } from "../../../src/utils/startsWith";
import { Show } from "../../../src/input/outputArgs";
import { reduceCmd } from "../../../src/commands/reduceCmd";
import printf from "printf";

type FBalArgs = DbArgs & {
  account?: string;
};
const fbalArgs = baseArgs.options({
  account: {
    type: "string",
    alias: "a",
    description: "Account to show",
  },
});

function fix(n: number) {
  const fixed = n.toFixed(2);
  // turn -0 into 0
  return fixed.match(/^-0(\.0+)$/) ? fixed.slice(1) : fixed;
}

export async function fbal(args: FBalArgs): Promise<void> {
  args.db ??= "finance";

  const startEnd = args.account
    ? {
        start: `,${args.account}`,
      }
    : {
        start: ",A",
        end: `,Z${HIGH_STRING}`,
      };

  const allBalances = (
    await reduceCmd({
      ...args,
      mapName: balanceView.name,
      ...startEnd,
      groupLevel: 2,
      show: Show.None,
    })
  ).rows;
  const { accountWidth, currencyWidth, amountWidth } = allBalances.reduce(
    (acc, row) => {
      const account = row.key[0];
      const currency = row.key[1];
      const amount = row.value;
      acc["accountWidth"] = Math.max(acc["accountWidth"], account.length);
      acc["currencyWidth"] = Math.max(acc["currencyWidth"], currency.length);
      acc["amountWidth"] = Math.max(acc["amountWidth"], fix(amount).length);
      return acc;
    },
    { accountWidth: 1, amountWidth: 1, currencyWidth: 1 },
  );

  const format = `%${accountWidth}.${accountWidth}s  %${amountWidth}.2f %-${currencyWidth}.${currencyWidth}s`;

  let previousAccount = "";
  allBalances.forEach((row) => {
    const amount = row.value;
    if (fix(amount) === fix(0)) {
      return; //TODO: Have option to show 0 balances as well
    }

    const account = row.key[0];
    const accountStr = account === previousAccount ? "" : account;
    previousAccount = account;

    const currency = row.key[1];
    console.log(printf(format, accountStr, amount, currency));
  });
}

if (require.main === module) {
  const args = fbalArgs.parseSync(process.argv.slice(2)) as FBalArgs;
  args.account ??= args?._?.[0] as string;
  fbal(args).catch((err) => {
    throw err;
  });
}
