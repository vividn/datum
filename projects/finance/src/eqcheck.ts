#!/usr/bin/env node

import { balanceView, equalityView } from "../views";
import { mapCmd } from "../../../src/commands/mapCmd";
import { reduceCmd } from "../../../src/commands/reduceCmd";
import { Show } from "../../../src/input/outputArgs";
import { transactionWatcher, transactionView } from "./transactionView";
import { DateTime } from "luxon";
import { ArgumentParser } from "argparse";
import { DbArgs, dbArgs } from "../../../src/input/dbArgs";
import { parseIfNeeded } from "../../../src/utils/parseIfNeeded";
import { HIGH_STRING } from "../../../src/utils/startsWith";

export const zeroDate = "0000-00-00";

type EqCheckArgs = DbArgs & {
  account?: string;
  watch?: boolean;
  context?: number;
  decimals?: number;
};

const eqCheckArgs = new ArgumentParser({
  description: "Check that the balances of accounts matches equality checks",
  add_help: true,
  prog: "eqcheck",
  parents: [dbArgs],
});
eqCheckArgs.add_argument("--account", "-a", {
  type: "str",
  help: "Account to check",
});
eqCheckArgs.add_argument("--context", "-C", {
  type: "int",
  help: "days of context around the equality checks. default: 3",
});
eqCheckArgs.add_argument("--watch", "-w", {
  action: "store_true",
  help: "Watch for changes",
});
eqCheckArgs.add_argument("--decimals", {
  type: "int",
  help: "Number of decimals to show",
});
eqCheckArgs.set_defaults({ db: "finance", context: 3, decimals: 2 });

async function eqcheck(
  cliOrArgs: EqCheckArgs | string | string[],
): Promise<void> {
  const args = parseIfNeeded(eqCheckArgs, cliOrArgs);
  const start = args.account ? `,${args.account}` : undefined;
  const decimals = args.decimals;
  function fix(n: number) {
    const fixed = n.toFixed(decimals);
    // turn -0 into 0
    return fixed.match(/^-0(\.0+)$/) ? fixed.slice(1) : fixed;
  }

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
    const actualBalance =
      (
        await reduceCmd({
          ...args,
          mapName: balanceView.name,
          start: `,${account},${currency},${zeroDate}`,
          end: `,${account},${currency},"${datetime}",${HIGH_STRING}`,
          show: Show.None,
        })
      ).rows[0]?.value ?? 0;
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
          decimals,
        });
      } else {
        await transactionView({
          args,
          account,
          currency,
          endDate,
          startDate,
          decimals,
        });
        process.exit(3);
      }
    }
    lastGoodEquality = row.key;
    console.info(`Balance check passed for ${account} ${currency} ${datetime}`);
  }
}

if (require.main === module) {
  eqcheck(process.argv.slice(2)).catch((err) => {
    throw err;
  });
}
