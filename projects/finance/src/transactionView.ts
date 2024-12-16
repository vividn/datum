#!/usr/bin/env node

import { connectDb } from "../../../src/auth/connectDb";
import readline from "node:readline";
import { stdin } from "node:process";
import { once } from "node:events";
import { isoDate, isoDateOrTime } from "../../../src/time/timeUtils";
import { reduceCmd } from "../../../src/commands/reduceCmd";
import { balanceView, equalityView } from "../views";
import { Show } from "../../../src/input/outputArgs";
import { mapCmd } from "../../../src/commands/mapCmd";
import { EqDoc, TxDoc, XcDoc } from "../views/balance";
import chalk from "chalk";
import { zeroDate } from "./eqcheck";
import { HIGH_STRING } from "../../../src/utils/startsWith";
import { DateTime } from "luxon";
import { parseDateStr } from "../../../src/time/parseDateStr";
import { dbArgs, DbArgs } from "../../../src/input/dbArgs";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../../../src/utils/parseIfNeeded";
import Table from "cli-table3";
import { sprintf } from "sprintf-js";

type TransactionViewInput = {
  args: DbArgs;
  account: string;
  currency: string;
  endDate: isoDateOrTime;
  startDate?: isoDateOrTime;
  decimals?: number;
};

export async function transactionWatcher({
  args,
  account,
  currency,
  startDate = zeroDate,
  endDate,
  decimals = 2,
}: TransactionViewInput): Promise<void> {
  let isBalanced = false;
  async function output() {
    console.clear();
    isBalanced = await transactionView({
      args,
      account,
      currency,
      startDate: startDate,
      endDate: endDate,
      decimals,
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
  rl.on("line", async () => {
    if (isBalanced) {
      rl.close();
    } else {
      await output();
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

export async function transactionView({
  args,
  account,
  currency,
  endDate,
  startDate = zeroDate,
  decimals = 2,
}: TransactionViewInput): Promise<boolean> {
  function fix(n: number) {
    const fixed = n.toFixed(decimals);
    // turn -0 into 0
    return fixed.match(/^-0(\.0+)$/) ? fixed.slice(1) : fixed;
  }
  const startBalance =
    ((
      await reduceCmd({
        ...args,
        mapName: balanceView.name,
        start: `,${account},${currency},${zeroDate}`,
        end: `,${account},${currency},${startDate}`,
        params: { inclusive_end: false },
        show: Show.None,
      })
    ).rows[0]?.value as number) ?? 0;
  const endBalance =
    (
      await reduceCmd({
        ...args,
        mapName: balanceView.name,
        start: `,${account},${currency},${zeroDate}`,
        end: `,${account},${currency},${endDate},${HIGH_STRING}`,
        show: Show.None,
      })
    ).rows[0]?.value ?? 0;
  const transactions = (
    await mapCmd({
      ...args,
      mapName: balanceView.name,
      start: `,${account},${currency},"${startDate}"`,
      end: `,${account},${currency},${endDate},${HIGH_STRING}`,
      show: Show.None,
      reverse: true,
      params: { include_docs: true },
    })
  ).rows as PouchDB.Query.Response<TxDoc | XcDoc>["rows"];
  const equalities = (
    await mapCmd({
      ...args,
      mapName: equalityView.name,
      start: `,${account},${currency},"${startDate}"`,
      end: `,${account},${currency},"${endDate}"`,
      show: Show.None,
      reverse: true,
      params: { include_docs: true },
    })
  ).rows as PouchDB.Query.Response<EqDoc>["rows"];

  console.log(chalk.yellow.bold(`${account} ${currency}`));

  const width = Math.max(Math.min(80, process.stdout.columns), 30);
  const dateWidth = 10;
  const hidWidth = 4;
  const toAccountWidth = 10;
  const arrowWidth = 1;
  const amountWidth =
    Math.floor(
      Math.log10(
        Math.max(
          1,
          ...transactions.map((row) => Math.abs(row.value)),
          Math.abs(endBalance - (equalities[0]?.value ?? endBalance)),
        ),
      ),
    ) +
    3 +
    decimals;
  const runningTotalWidth =
    Math.floor(
      Math.log10(
        Math.max(
          1,
          transactions.reduce(
            (accum: { runningTotal: number; absMax: number }, current) => {
              const runningTotal = accum.runningTotal - current.value;
              return {
                runningTotal,
                absMax: Math.max(accum.absMax, Math.abs(runningTotal)),
              };
            },
            { runningTotal: endBalance, absMax: Math.abs(endBalance) },
          ).absMax,
        ),
      ),
    ) +
    3 +
    decimals;
  const commentWidth =
    width -
    dateWidth -
    hidWidth -
    toAccountWidth -
    arrowWidth -
    amountWidth -
    runningTotalWidth -
    6;

  const table = new Table({
    head: ["Date", "HID", "Comment", "To Account", "↔", "Amount", "Balance"],
    colAligns: ["left", "left", "left", "right", "center", "right", "right"],
    style: {
      head: ["yellow"],
      border: ["grey"],
      "padding-left": 0,
      "padding-right": 0,
    },
    chars: {
      top: "",
      "top-mid": "",
      "top-left": "",
      "top-right": "",
      bottom: "",
      "bottom-mid": "",
      "bottom-left": "",
      "bottom-right": "",
      left: "",
      "left-mid": "",
      mid: "",
      "mid-mid": "",
      right: "",
      "right-mid": "",
      middle: " ",
    },
    colWidths: [
      dateWidth,
      hidWidth,
      commentWidth,
      toAccountWidth,
      arrowWidth,
      amountWidth,
      runningTotalWidth,
    ],
  });

  let reverseBalance = endBalance;
  let isAllBalanced = true;

  function displayEquality(
    equality: PouchDB.Query.Response<EqDoc>["rows"][0],
    currentBalance: number,
  ): boolean {
    const date = equality.key[2].slice(0, dateWidth);
    const hid = equality.doc?.meta?.humanId?.slice(0, hidWidth) ?? "";
    const eqBalance = equality.value;
    const amount = eqBalance - currentBalance;
    const formattedAmount = sprintf(
      `%${amountWidth}.${decimals}f`,
      fix(amount),
    );
    const formattedEqBalance = sprintf(
      `%${runningTotalWidth}.${decimals}f`,
      fix(eqBalance),
    );
    const isBalanced = fix(amount) === fix(0);

    const row = [
      date,
      hid,
      isBalanced ? "EqCheck" : "FAIL",
      "",
      "",
      formattedAmount,
      formattedEqBalance,
    ];

    table.push(
      row.map((cell) => (isBalanced ? chalk.green(cell) : chalk.red(cell))),
    );

    return isBalanced;
  }

  function displayTransaction(
    transaction: PouchDB.Query.Response<TxDoc | XcDoc>["rows"][0],
    currentBalance: number,
  ): number {
    const doc = transaction.doc!;
    const {
      data: { comment = "" },
    } = doc;
    const hid = doc.meta?.humanId?.slice(0, hidWidth) ?? "";
    const amount = transaction.value;
    const formattedAmount = sprintf(
      `%${amountWidth}.${decimals}f`,
      fix(amount),
    );
    const formattedBalance = sprintf(
      `%${runningTotalWidth}.${decimals}f`,
      fix(currentBalance),
    );
    const toAccount = transaction.key[3];
    const arrow = amount > 0 ? "→" : "←";
    const date = transaction.key[2].slice(0, dateWidth);

    table.push([
      date,
      hid,
      comment.toString(),
      toAccount,
      arrow,
      formattedAmount,
      formattedBalance,
    ]);

    return amount;
  }

  while (transactions.length || equalities.length) {
    const transactionDate = transactions[0]?.key[2] ?? zeroDate;
    const equalityDate = equalities[0]?.key[2] ?? zeroDate;
    if (
      !transactions.length ||
      (equalityDate >= transactionDate && equalities.length)
    ) {
      const balanced = displayEquality(equalities.shift()!, reverseBalance);
      isAllBalanced &&= balanced;
    } else {
      reverseBalance -= displayTransaction(
        transactions.shift()!,
        reverseBalance,
      );
    }
  }

  if (fix(reverseBalance) === "-0.00") {
    reverseBalance = 0;
  }
  if (startDate === zeroDate) {
    displayEquality(
      {
        key: [account, currency, zeroDate],
        value: 0,
        id: "",
        doc: undefined,
      },
      reverseBalance,
    );
  }

  console.log(table.toString());

  if (fix(reverseBalance) !== fix(startBalance)) {
    throw new Error(
      `Running total does not align with data. Something went wrong. Expected: ${fix(
        startBalance,
      )}, got ${fix(reverseBalance)} (${fix(reverseBalance - startBalance)})`,
    );
  }
  return isAllBalanced;
}

const transactionViewArgs = new ArgumentParser({
  description: "View transactions for an account",
  prog: "txview",
  usage: `%(prog)s <account> <currency> [start] [end]`,
  parents: [dbArgs],
});
transactionViewArgs.add_argument("account", {
  help: "The account to view",
});
transactionViewArgs.add_argument("currency", {
  help: "The currency to view",
});
transactionViewArgs.add_argument("start", {
  help: "The start date to view",
  nargs: "?",
});
transactionViewArgs.add_argument("end", {
  help: "The end date to view",
  nargs: "?",
});
transactionViewArgs.set_defaults({ db: "finance" });

type TxViewArgs = DbArgs & {
  account: string;
  currency: string;
  start?: string;
  end?: string;
};

async function transactionViewCmd(argsOrCli: TxViewArgs | string | string[]) {
  const args = parseIfNeeded(transactionViewArgs, argsOrCli);
  const { account, currency, start, end } = args;
  const startDate: isoDate = start
    ? (parseDateStr({ dateStr: start }).toISODate() as string)
    : zeroDate;
  const endDate: isoDate = end
    ? (parseDateStr({ dateStr: end }).toISODate() as string)
    : (DateTime.now().toISODate() as string);
  await transactionView({
    args,
    account,
    currency,
    startDate,
    endDate,
  });
}

if (require.main === module) {
  transactionViewCmd(process.argv.slice(2)).catch((err) => {
    throw err;
  });
}
