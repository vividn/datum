#!/usr/bin/env node

import { BaseArgs, baseArgs } from "../../../src/input/baseArgs";
import { connectDb } from "../../../src/auth/connectDb";
import { balanceView, equalityView } from "../views";
import { mapCmd } from "../../../src/commands/mapCmd";
import { reduceCmd } from "../../../src/commands/reduceCmd";
import { Show } from "../../../src/input/outputArgs";

async function main(cliInput: string | string[]) {
  const args = (await baseArgs.parse(cliInput)) as BaseArgs;
  args.db ??= "finance";
  const db = connectDb(args);
  const allEqualityChecks = (
    await mapCmd({ ...args, mapName: equalityView.name, show: Show.None })
  ).rows;

  for (const row of allEqualityChecks) {
    const [account, currency, datetime] = row.key;
    const expectedBalance = row.value.toFixed(2);
    const actualBalance = (
      await reduceCmd({
        ...args,
        mapName: balanceView.name,
        start: `[${account}, ${currency}, "0"]`,
        end: `[${account}, ${currency}, "${datetime}"]`,
        show: Show.None,
      })
    ).rows[0].value.toFixed(2);
    if (expectedBalance !== actualBalance) {
      console.error(
        `Balance mismatch for ${account} ${currency} ${datetime}: expected ${expectedBalance}, got ${actualBalance}`
      );
      process.exit(3);
    }
    console.info(`Balance check passed for ${account} ${currency} ${datetime}`);
  }
}

if (require.main === module) {
  main(process.argv.slice(2)).catch((err) => {
    console.error(err);
  });
}
