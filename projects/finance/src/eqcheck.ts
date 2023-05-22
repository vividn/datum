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
import { connectDb } from "../../../src/auth/connectDb";
import * as readline from "node:readline";
import { stdin } from "node:process";
import { once } from "node:events";

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
      if (args.watch) {
        await balanceWatcher({
          args,
          account,
          currency,
          failDate: datetime,
          goodDate: lastGoodDate,
        });
      } else {
        await transactionView({
          args,
          account,
          currency,
          failDate: datetime,
          goodDate: lastGoodDate,
        });
        process.exit(3);
      }
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
  let isBalanced = false;
  async function output() {
    console.clear();
    isBalanced = await transactionView({
      args,
      account,
      currency,
      goodDate,
      failDate,
    });
  }

  const db = connectDb(args);
  const eventEmitter = db
    .changes({ since: "now", live: true })
    .on("change", output)
    .on("error", (error) => {
      console.error(error);
      process.exit(5);
    });
  const rl = readline.createInterface({ input: stdin, terminal: true });
  stdin.setRawMode(true);
  rl.on("line", output);
  rl.on("line", () => {
    if (isBalanced) {
      rl.close();
    }
  });
  rl.on("close", () => {
    stdin.setRawMode(false);
  });
  rl.on("SIGINT", () => {
    rl.close();
    eventEmitter.cancel();
    process.exit(3);
  });

  await output();
  await once(rl, "close");
  eventEmitter.cancel();
  return;
}

async function transactionView({
  args,
  account,
  currency,
  goodDate = zeroDate,
  failDate,
}: BalanceWatcherInput): Promise<boolean> {
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
  const goodEqualityDoc = (
    await mapCmd({
      ...args,
      mapName: equalityView.name,
      start: `,${account},${currency},${goodDate}`,
      show: Show.None,
      params: { include_docs: true },
    })
  ).rows[0];
  const goodEquality = goodEqualityDoc?.value as number ?? 0;
  const goodHid = (goodEqualityDoc?.doc?.meta?.humanId as string) ?? "";
  const failBalance = (
    await reduceCmd({
      ...args,
      mapName: balanceView.name,
      start: `,${account},${currency},${zeroDate}`,
      end: `,${account},${currency},"${failDate}"`,
      show: Show.None,
    })
  ).rows[0].value;

  const expectedEquality = (
    await mapCmd({
      ...args,
      mapName: equalityView.name,
      start: `,${account},${currency},${failDate}`,
      show: Show.None,
      params: { include_docs: true },
    })
  ).rows[0];
  const expectedBalance = expectedEquality.value;
  const expectedEqualityHid =
    (expectedEquality.doc?.meta?.humanId as string) ?? "";
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

  const width = Math.max(Math.min(80, process.stdout.columns), 30);
  const dateWidth = 10;
  const hidWidth = 4;
  const toAccountWidth = 10;
  const arrowWidth = 1;
  const amountWidth =
    Math.ceil(
      Math.log10(Math.max(1, ...transactions.map((row) => Math.abs(row.value))))
    ) + 4;
  const runningTotalWidth =
    Math.ceil(
      Math.log10(Math.max(Math.abs(goodBalance), Math.abs(failBalance)))
    ) + 4;
  const commentWidth =
    width -
    dateWidth -
    hidWidth -
    toAccountWidth -
    arrowWidth -
    amountWidth -
    runningTotalWidth -
    6;
  const formatString =
    `%-${dateWidth}.${dateWidth}s ` +
    `%-${hidWidth}.${hidWidth}s ` +
    `%-${commentWidth}.${commentWidth}s ` +
    `%${toAccountWidth}.${toAccountWidth}s ` +
    `%${arrowWidth}.${arrowWidth}s ` +
    `%${amountWidth}.2f ` +
    `%${runningTotalWidth}.2f`;

  console.clear();
  console.log(chalk.yellow.bold(`${account} ${currency}`));
  const isBalanced = fix(expectedBalance) === fix(failBalance);
  console.log(
    isBalanced
      ? chalk.greenBright(
          printf(
            formatString,
            failDate,
            expectedEqualityHid,
            "EqCheck",
            "",
            "",
            expectedBalance - failBalance,
            expectedBalance
          )
        )
      : chalk.redBright(
          printf(
            formatString,
            failDate,
            expectedEqualityHid,
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
    const hid = doc.meta?.humanId ?? "";
    const amount = row.value;
    const toAccount = row.key[3];
    const arrow = amount > 0 ? "→" : "←";
    const date = row.key[2];
    console.log(
      printf(
        formatString,
        date,
        hid,
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
    fix(goodEquality) === fix(reverseBalance)
      ? chalk.greenBright(
          printf(
            formatString,
            goodDate,
            goodHid,
            "EqCheck",
            "",
            "",
            0,
            goodBalance
          ).replace(/0\.00/, "    ")
        )
      : chalk.redBright(
          printf(
            formatString,
            goodDate,
            goodHid,
            "FAIL",
            "",
            "",
            goodBalance - goodEquality,
            reverseBalance
          )
        )
  );

  if (fix(reverseBalance) !== fix(goodBalance)) {
    console.warn(
      chalk.red.bold(
        `Running total does not align with data. Something went wrong. Expected: ${fix(
          goodBalance
        )}, got ${fix(reverseBalance)} (${fix(reverseBalance - goodBalance)})`
      )
    );
  }

  if (fix(goodBalance) !== fix(goodEquality)) {
    console.warn(
      chalk.red.bold(`Initial balance has changed. May want to rerun.`)
    );
  }
  return isBalanced;
}

if (require.main === module) {
  main(process.argv.slice(2)).catch((err) => {
    console.error(err);
  });
}
