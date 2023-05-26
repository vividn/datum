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

  const goodEqualityDoc = (
    await mapCmd({
      ...args,
      mapName: equalityView.name,
      start: `,${account},${currency},${startDate}`,
      show: Show.None,
      params: { include_docs: true },
    })
  ).rows[0];
  const goodEquality = (goodEqualityDoc?.value as number) ?? 0;
  const goodHid = (goodEqualityDoc?.doc?.meta?.humanId as string) ?? "";

  const expectedEquality = (
    await mapCmd({
      ...args,
      mapName: equalityView.name,
      start: `,${account},${currency},${endDate}`,
      show: Show.None,
      params: { include_docs: true },
    })
  ).rows[0];
  const expectedBalance = expectedEquality.value;
  const expectedEqualityHid =
    (expectedEquality.doc?.meta?.humanId as string) ?? "";

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
  const formatString =
    `%-${dateWidth}.${dateWidth}s ` +
    `%-${hidWidth}.${hidWidth}s ` +
    `%-${commentWidth}.${commentWidth}s ` +
    `%${toAccountWidth}.${toAccountWidth}s ` +
    `%${arrowWidth}.${arrowWidth}s ` +
    `%${amountWidth}.2f ` +
    `%${runningTotalWidth}.2f`;

  console.log(chalk.yellow.bold(`${account} ${currency}`));
  const isBalanced = fix(expectedBalance) === fix(endBalance);
  console.log(
    isBalanced
      ? chalk.greenBright(
          printf(
            formatString,
            endDate,
            expectedEqualityHid,
            "EqCheck",
            "",
            "",
            expectedBalance - endBalance,
            expectedBalance
          )
        )
      : chalk.redBright(
          printf(
            formatString,
            endDate,
            expectedEqualityHid,
            "FAIL",
            "",
            "",
            expectedBalance - endBalance,
            expectedBalance
          )
        )
  );
  let reverseBalance = endBalance;
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
            startDate,
            goodHid,
            "EqCheck",
            "",
            "",
            0,
            startBalance
          ).replace(/0\.00/, "    ")
        )
      : chalk.redBright(
          printf(
            formatString,
            startDate,
            goodHid,
            "FAIL",
            "",
            "",
            startBalance - goodEquality,
            reverseBalance
          )
        )
  );

  if (fix(reverseBalance) !== fix(startBalance)) {
    console.warn(
      chalk.red.bold(
        `Running total does not align with data. Something went wrong. Expected: ${fix(
          startBalance
        )}, got ${fix(reverseBalance)} (${fix(reverseBalance - startBalance)})`
      )
    );
  }

  if (fix(startBalance) !== fix(goodEquality)) {
    console.warn(
      chalk.red.bold(`Initial balance has changed. May want to rerun.`)
    );
  }
  return isBalanced;
}
