#!/usr/bin/env node

import { choreView } from "../views";
import { reduceCmd } from "../../../src/commands/reduceCmd";
import chalk from "chalk";
import { DateTime } from "luxon";
import { Show } from "../../../src/input/outputArgs";
import { ReduceRow } from "../../../src/views/DatumView";

function choreRowToLastNextChore(row: ReduceRow<typeof choreView>): string {
  const { key, value } = row;
  const last =
    value.lastOccur === "0000-00-00"
      ? "##NEVER## "
      : value.lastOccur.slice(0, 10);
  const next = value.next ? value.next.slice(0, 10) : "INACTIVE";
  return [last, next, key].join("\t");
}
async function nextchores() {
  const choresResponse = await reduceCmd({
    mapName: choreView.name,
    groupLevel: 1,
    show: Show.None,
  });
  const chores = choresResponse.rows
    .filter((chore) => chore.value.next !== undefined)
    .sort((a, b) => a.value.next - b.value.next);
  const now = DateTime.local().toISODate();
  const due = chores.filter((row) => row.value.next <= now);
  const future = chores.filter((row) => row.value.next > now);
  const tenDash = "----------";
  const header = "Last Done \tNext Date \tChore";
  console.log(header);
  if (due.length === 0) {
    if (process.stdout.isTTY) {
      console.log(chalk.green("COMPLETED!\tGOOD WORK!\t\\(◦'⌣'◦)/"));
    }
  } else {
    console.log(
      chalk.red(due.map((row) => choreRowToLastNextChore(row)).join("\n")),
    );
    if (process.stdout.isTTY) {
      console.log(tenDash + "\t" + tenDash + "\t" + tenDash);
    }
  }
  const futureColor = due.length === 0 ? chalk.green : chalk.white;
  console.log(
    futureColor(future.map((row) => choreRowToLastNextChore(row)).join("\n")),
  );
}

if (require.main === module) {
  nextchores().catch((error) => {
    console.error(error);
    process.exit(1);
  });
}
