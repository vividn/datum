#!/usr/bin/env node

import { BaseArgs, baseArgs } from "../../../src/input/baseArgs";
import { balanceView, equalityView } from "../views";
import { mapCmd } from "../../../src/commands/mapCmd";
import { reduceCmd } from "../../../src/commands/reduceCmd";
import { Show } from "../../../src/input/outputArgs";
import { transactionWatcher, transactionView } from "./transactionView";
import { DateTime } from "luxon";

export const zeroDate = "0000-00-00";

export function fix(n: number) {
  return n.toFixed(2);
}

type EqCheckArgs = BaseArgs & {
  account?: string;
  watch?: boolean;
  context?: number;
};

const eqCheckYargs = baseArgs
  .option("account", {
    alias: "a",
    type: "string",
    description: "Account to check",
  })
  .option("context", {
    alias: "C",
    type: "number",
    description: "days of context around the equality checks. default: 3",
  })
  .option("watch", {
    alias: "w",
    type: "boolean",
    description: "Watch for changes",
  });

async function main(cliInput: string | string[]) {
  const args = (await eqCheckYargs.parse(cliInput)) as EqCheckArgs;
  args.db ??= "finance";
  args.context ??= 3;
  const start = args.account ? `,${args.account}` : undefined;
  const allEqualityChecks = (
    await mapCmd({
      ...args,
      mapName: equalityView.name,
      start,
      show: Show.None,
    })
  ).rows;
  let lastGoodEquality = [undefined, undefined, undefined];
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
      const lastGoodDate =
        account === lastGoodEquality[0] && currency === lastGoodEquality[1]
          ? lastGoodEquality[2]
          : undefined;
      const startDate = lastGoodDate
        ? (DateTime.fromISO(lastGoodDate)
            .minus({ days: args.context })
            .toISODate() as string)
        : undefined;
      const endDate = DateTime.fromISO(datetime)
        .plus({ days: args.context })
        .toISODate() as string;
      if (args.watch) {
        await transactionWatcher({
          args,
          account,
          currency,
          endDate,
          startDate,
        });
      } else {
        await transactionView({
          args,
          account,
          currency,
          endDate,
          startDate,
        });
        process.exit(3);
      }
    }
    lastGoodEquality = row.key;
    console.info(`Balance check passed for ${account} ${currency} ${datetime}`);
  }
}

if (require.main === module) {
  main(process.argv.slice(2)).catch((err) => {
    console.error(err);
  });
}
