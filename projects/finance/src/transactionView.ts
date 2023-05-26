import { connectDb } from "../../../src/auth/connectDb";
import readline from "node:readline";
import { stdin } from "node:process";
import { once } from "node:events";
import { BaseArgs } from "../../../src/input/baseArgs";
import { isoDateOrTime } from "../../../src/time/timeUtils";
import { reduceCmd } from "../../../src/commands/reduceCmd";
import { balanceView, equalityView } from "../views";
import { Show } from "../../../src/input/outputArgs";
import { mapCmd } from "../../../src/commands/mapCmd";
import { EqDoc, TxDoc, XcDoc } from "../views/balance";
import chalk from "chalk";
import printf from "printf";
import { fix, zeroDate } from "./eqcheck";

type TransactionViewInput = {
  args: BaseArgs;
  account: string;
  currency: string;
  endDate: isoDateOrTime;
  startDate?: isoDateOrTime;
};

export async function transactionWatcher({
  args,
  account,
  currency,
  startDate = zeroDate,
  endDate,
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

export async function transactionView({
  args,
  account,
  currency,
  endDate,
  startDate = zeroDate,
}: TransactionViewInput): Promise<boolean> {
  const startBalance =
    ((
      await reduceCmd({
        ...args,
        mapName: balanceView.name,
        start: `,${account},${currency},${zeroDate}`,
        end: `,${account},${currency},${startDate}`,
        show: Show.None,
      })
    ).rows[0]?.value as number) ?? 0;
  const endBalance = (
    await reduceCmd({
      ...args,
      mapName: balanceView.name,
      start: `,${account},${currency},${zeroDate}`,
      end: `,${account},${currency},"${endDate}"`,
      show: Show.None,
    })
  ).rows[0].value;
  const transactions = (
    await mapCmd({
      ...args,
      mapName: balanceView.name,
      start: `,${account},${currency},"${startDate}"`,
      end: `,${account},${currency},"${endDate}"`,
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
      Math.log10(
        Math.max(
          Math.abs(startBalance),
          Math.abs(endBalance),
          ...equalities.map((row) => Math.abs(row.value))
        )
      )
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
  const format =
    `%-${dateWidth}.${dateWidth}s ` +
    `%-${hidWidth}.${hidWidth}s ` +
    `%-${commentWidth}.${commentWidth}s ` +
    `%${toAccountWidth}.${toAccountWidth}s ` +
    `%${arrowWidth}.${arrowWidth}s ` +
    `%${amountWidth}.2f ` +
    `%${runningTotalWidth}.2f`;

  console.log(chalk.yellow.bold(`${account} ${currency}`));
  let reverseBalance = endBalance;
  let isAllBalanced = true;

  function displayEquality(
    equality: PouchDB.Query.Row<EqDoc>,
    currentBalance: number
  ): boolean {
    const date = equality.key[2];
    const hid = equality.doc?.meta?.humanId ?? "";
    const eqBalance = equality.value;
    const amount = eqBalance - currentBalance;
    const isBalanced = fix(eqBalance) === fix(currentBalance);
    console.log(
      isBalanced
        ? chalk.greenBright(
            printf(format, date, hid, "EqCheck", "", "", 0, eqBalance).replace(
              /0\.00/,
              "    "
            )
          )
        : chalk.redBright(
            printf(format, date, hid, "FAIL", "", "", amount, eqBalance)
          )
    );
  }
  function displayTransaction(
    transaction: PouchDB.Query.Row<TxDoc | XcDoc>,
    currentBalance: number
  ): number {
    const doc = transaction.doc!;
    const {
      data: { comment = "" },
    } = doc;
    const hid = doc.meta?.humanId ?? "";
    const amount = transaction.value;
    const toAccount = transaction.key[3];
    const arrow = amount > 0 ? "→" : "←";
    const date = transaction.key[2];
    console.log(
      printf(
        format,
        date,
        hid,
        comment,
        toAccount,
        arrow,
        amount,
        currentBalance
      )
    );
    return amount;
  }
  while (transactions.length || equalities.length) {
    const transactionDate = transactions[0]?.key[2] ?? zeroDate;
    const equalityDate = equalities[0]?.key[2] ?? zeroDate;

    if (equalityDate >= transactionDate) {
      isAllBalanced &&= displayEquality(equalities.shift()!, reverseBalance);
    } else {
      reverseBalance -= displayTransaction(
        transactions.shift()!,
        reverseBalance
      );
    }
  }
  if (startDate === zeroDate) {
    displayEquality({
      key: [account, currency, zeroDate],
      value: 0,
      doc: null,
    });
  }

  if (fix(reverseBalance) !== fix(startBalance)) {
    throw new Error(
      `Running total does not align with data. Something went wrong. Expected: ${fix(
        startBalance
      )}, got ${fix(reverseBalance)} (${fix(reverseBalance - startBalance)})`
    );
  }
  return isAllBalanced;
}
