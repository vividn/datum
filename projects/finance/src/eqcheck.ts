#!/usr/bin/env node

import { BaseArgs, baseArgs } from "../../../src/input/baseArgs";
import { balanceView, equalityView } from "../views";
import { mapCmd } from "../../../src/commands/mapCmd";
import { reduceCmd } from "../../../src/commands/reduceCmd";
import { Show } from "../../../src/input/outputArgs";
import { isoDateOrTime } from "../../../src/time/timeUtils";
import { TxDoc, XcDoc } from "../views/balance";
import printf from "printf";
import chalk from "chalk";

const zeroDate = "0000-00-00";

function fix(n: number) {
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
      await balanceWatcher({
        args,
        account,
        currency,
        failDate: datetime,
        goodDate: lastGoodDate,
      });
      process.exit(3);
      // console.error(
      //   `Balance mismatch for ${account} ${currency} ${datetime}:\nexpected ${fix(
      //     expectedBalance
      //   )}, got ${fix(actualBalance)} (${actualBalance - expectedBalance})`
      // );
      // process.exit(3);
    }
    lastGoodDate = datetime;
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
  goodDate = zeroDate,
  failDate,
}: BalanceWatcherInput) {
  let initialGoodBalance: number | undefined = undefined;
  console.log({
    ...args,
    mapName: balanceView.name,
    start: `,${account},${currency},${zeroDate}`,
    end: `,${account},${currency},${goodDate}`,
    show: Show.None,
  });

  const goodBalance =
    ((
      await reduceCmd({
        ...args,
        mapName: balanceView.name,
        start: `,${account},${currency},${zeroDate}`,
        end: `,${account},${currency},${goodDate}`,
        show: Show.None,
      })
    ).rows[0]?.value as number) ?? 0;
  initialGoodBalance ??= goodBalance;
  const failBalance = (
    await reduceCmd({
      ...args,
      mapName: balanceView.name,
      start: `,${account},${currency},${zeroDate}`,
      end: `,${account},${currency},"${failDate}"`,
      show: Show.None,
    })
  ).rows[0].value;

  const expectedBalanceMap = await mapCmd({
    ...args,
    mapName: equalityView.name,
    start: `,${account},${currency},${failDate}`,
    show: Show.None,
  });
  const expectedBalance = expectedBalanceMap.rows[0].value;
  const transactions = (
    await mapCmd({
      ...args,
      mapName: balanceView.name,
      start: `,${account},${currency},"${goodDate}"`,
      end: `,${account},${currency},"${failDate}"`,
      show: Show.None,
      reverse: true,
      params: { include_docs: true },
    })
  ).rows as PouchDB.Query.Response<TxDoc | XcDoc>["rows"];

  const width = Math.max(Math.min(70, process.stdout.columns), 30);
  const dateWidth = 10;
  const toAccountWidth = 10;
  const arrowWidth = 1;
  const amountWidth =
    Math.ceil(
      Math.log10(Math.max(...transactions.map((row) => Math.abs(row.value))))
    ) + 4;
  const runningTotalWidth =
    Math.ceil(
      Math.log10(
        Math.max(Math.abs(initialGoodBalance ?? 0), Math.abs(failBalance))
      )
    ) + 4;
  const commentWidth =
    width -
    dateWidth -
    toAccountWidth -
    arrowWidth -
    amountWidth -
    runningTotalWidth -
    4;
  const formatString = `%-${dateWidth}.${dateWidth}s %-${commentWidth}.${commentWidth}s %${toAccountWidth}.${toAccountWidth}s %${arrowWidth}.${arrowWidth}s %${amountWidth}.2f %${runningTotalWidth}.2f`;

  console.clear();
  console.log(`${account} ${currency}`);
  console.log(
    chalk.redBright(
      printf(
        formatString,
        failDate,
        "FAIL",
        "",
        "",
        expectedBalance - failBalance,
        expectedBalance
      )
    )
  );
  let reverseBalance = failBalance;
  for (const row of transactions) {
    const doc = row.doc!;
    const {
      data: { comment = "" },
    } = doc;
    const amount = row.value;
    const toAccount = row.key[3];
    const arrow = amount > 0 ? "→" : "←";
    const date = row.key[2];
    console.log(
      printf(
        formatString,
        date,
        comment,
        toAccount,
        arrow,
        amount,
        reverseBalance
      )
    );
    reverseBalance -= amount;
  }

  console.log(
    chalk.greenBright(
      printf(
        formatString,
        goodDate,
        "EqCheck",
        "",
        "",
        0,
        goodBalance
      ).replace(/0\.00/, "    ")
    )
  );

  // if (fix(reverseBalance) !== fix(goodBalance)) {
  //   console.warn(
  //     `Running total does not align with data. Something went wrong. Expected: ${fix(
  //       goodBalance
  //     )}, got ${fix(reverseBalance)} (${fix(reverseBalance - goodBalance)})`
  //   );
  // }
  // if (fix(goodBalance) !== fix(initialGoodBalance)) {
  //   console.warn(`Initial balance has changed. May want to rerun.`);
  // }
}

if (require.main === module) {
  main(process.argv.slice(2)).catch((err) => {
    console.error(err);
  });
}
