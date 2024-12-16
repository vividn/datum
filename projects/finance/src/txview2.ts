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
  const format =
    `%-${dateWidth}.${dateWidth}s ` +
    `%-${hidWidth}.${hidWidth}s ` +
    `%-${commentWidth}.${commentWidth}s ` +
    `%${toAccountWidth}.${toAccountWidth}s ` +
    `%${arrowWidth}.${arrowWidth}s ` +
    `%${amountWidth}.${decimals}f ` +
    `%${runningTotalWidth}.${decimals}f`;
  console.log(chalk.yellow.bold(`${account} ${currency}`));
  let reverseBalance = endBalance;
  let isAllBalanced = true;

  function displayEquality(
    equality: PouchDB.Query.Response<EqDoc>["rows"][0],
    currentBalance: number,
  ): boolean {
    const date = equality.key[2];
    const hid = equality.doc?.meta?.humanId ?? "";
    const eqBalance = equality.value;
    const amount = eqBalance - currentBalance;
    const isBalanced = fix(amount) === fix(0);
    console.log(
      isBalanced
        ? chalk.green(
            printf(format, date, hid, "EqCheck", "", "", 0, eqBalance).replace(
              / 0\.0+ /,
              (match) => {
                // replace with equal number of spaces
                return " ".repeat(match.length);
              },
            ),
          )
        : chalk.red(
            printf(format, date, hid, "FAIL", "", "", amount, eqBalance),
          ),
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
        comment.toString(),
        toAccount,
        arrow,
        amount,
        currentBalance,
      ),
    );
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

  if (fix(reverseBalance) !== fix(startBalance)) {
    throw new Error(
      `Running total does not align with data. Something went wrong. Expected: ${fix(
        startBalance,
      )}, got ${fix(reverseBalance)} (${fix(reverseBalance - startBalance)})`,
    );
  }
  return isAllBalanced;
}


