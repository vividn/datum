#!/usr/bin/env node

import { BaseArgs, baseArgs } from "../../../src/input/baseArgs";
import { balanceView, equalityView } from "../views";
import { mapCmd } from "../../../src/commands/mapCmd";
import { reduceCmd } from "../../../src/commands/reduceCmd";
import { Show } from "../../../src/input/outputArgs";
import { transactionWatcher, transactionView } from "./transactionView";

export const zeroDate = "0000-00-00";

export function fix(n: number) {
  return n.toFixed(2);
}

type EqCheckArgs = BaseArgs & {
  account?: string;
  watch?: boolean;
};

const eqCheckYargs = baseArgs
  .option("account", {
    alias: "a",
    type: "string",
    description: "Account to check",
  })
  .option("watch", {
    alias: "w",
    type: "boolean",
    description: "Watch for changes",
  });

async function main(cliInput: string | string[]) {
  const args = (await eqCheckYargs.parse(cliInput)) as EqCheckArgs;
  args.db ??= "finance";
  const start = args.account ? `,${args.account}` : undefined;
  const allEqualityChecks = (
    await mapCmd({
      ...args,
      mapName: equalityView.name,
      start,
      show: Show.None,
    })
  ).rows;
  let lastGoodDate = undefined;
  for (const row of allEqualityChecks) {
    const [account, currency, datetime] = row.key;
    const expectedBalance = row.value;
    const actualBalance = (
      await reduceCmd({
        ...args,
        mapName: balanceView.name,
        start: `,${account},${currency},${zeroDate}`,
        end: `,${account},${currency},"${datetime}"`,
        show: Show.None,
      })
    ).rows[0].value;
    if (fix(expectedBalance) !== fix(actualBalance)) {
      if (args.watch) {
        await transactionWatcher({
          args,
          account,
          currency,
          endDate: datetime,
          startDate: lastGoodDate,
        });
      } else {
        await transactionView({
          args,
          account,
          currency,
          endDate: datetime,
          startDate: lastGoodDate,
        });
        process.exit(3);
      }
    }
    lastGoodDate = datetime;
    console.info(`Balance check passed for ${account} ${currency} ${datetime}`);
  }
}

if (require.main === module) {
  main(process.argv.slice(2)).catch((err) => {
    console.error(err);
  });
}
