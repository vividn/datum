#!/usr/bin/env node

import { reduceCmd } from "../../../src/commands/reduceCmd";
import { choreView } from "../views";
import { Show } from "../../../src/input/outputArgs";
import { DateTime } from "luxon";
import chalk from "chalk";
import { ReduceRow } from "../../../src/views/DatumView";
import Table from "easy-table";
import { BaseArgs, baseArgs } from "../../../src/input/baseArgs";
import { connectDb } from "../../../src/auth/connectDb";
import readline from "node:readline";
import { stdin, stdout } from "node:process";
import { once } from "node:events";
import { insertDatumView } from "../../../src/views/insertDatumView";

let oneTimeSetup = false;
async function chorelist(args: BaseArgs): Promise<string> {
  if (!oneTimeSetup) {
    const db = connectDb(args);
    await insertDatumView({
      db,
      datumView: choreView,
      outputArgs: { show: Show.Minimal },
    });
    oneTimeSetup = true;
  }
  const choresResponse = await reduceCmd({
    ...args,
    mapName: choreView.name,
    groupLevel: 1,
    show: Show.None,
  });
  const choreRows = choresResponse.rows as ReduceRow<typeof choreView>[];
  const chores = choreRows
    .filter((chore) => chore.value.next !== undefined)
    .sort((a, b) => (a.value.iti ?? 9e99) - (b.value.iti ?? 9e99));
  const now = DateTime.local().toISODate();

  const t = new Table();

  let totalDue = 0;
  chores.forEach((row) => {
    if (row.value.next === undefined) {
      return;
    }
    const next = row.value.next.slice(0, 10);
    const last =
      row.value.lastOccur === "0000-00-00"
        ? "NEVER"
        : DateTime.fromISO(row.value.lastOccur).toISODate();
    const due = next <= now;
    const doneToday = last === now;
    if (!due && !doneToday) {
      return;
    }
    if (due) {
      totalDue += 1;
    }
    const iti =
      row.value.iti === undefined ? undefined : Math.floor(row.value.iti);
    const daysOverdue = Math.floor(
      DateTime.fromISO(now).diff(DateTime.fromISO(next), "days").days,
    );
    const color = doneToday
      ? chalk.green
      : iti === undefined
        ? chalk.white
        : daysOverdue > iti
          ? chalk.red
          : daysOverdue > iti / 2
            ? chalk.yellow
            : chalk.white;

    t.cell("Field", color(row.key));
    t.cell("ITI", color(iti ?? ""));
    t.cell("Due", color(next));
    t.cell("Last Occur", color(last));
    t.newRow();
  });
  const choreTable = t.toString();
  const completionString =
    totalDue === 0 ? "\n🧹😄COMPLETED! GOOD WORK!😄🪣" : "";
  return choreTable + completionString;
}

async function chorelistwatch(args: BaseArgs): Promise<void> {
  const db = connectDb(args);

  async function output(): Promise<void> {
    const choreTable = await chorelist(args);
    console.clear();
    console.log(choreTable);
  }

  const rl = readline.createInterface({
    input: stdin,
    output: stdout,
    terminal: true,
  });
  stdin.setRawMode(true);

  const eventEmitter = db
    .changes({
      since: "now",
      live: true,
    })
    .on("change", output)
    .on("error", (error) => {
      console.error(error);
      rl.write("\u001B[?25h"); // show cursor
      rl.close();
      process.exit(5);
    });

  rl.on("line", async () => {
    console.log("refreshing...");
    await output();
  });
  rl.on("close", () => {
    rl.write("\u001B[?25h"); // show cursor
    stdin.setRawMode(false);
  });
  rl.on("SIGINT", () => {
    eventEmitter.cancel();
    rl.close();
    process.exit(3);
  });

  rl.write("\u001B[?25l"); // hides cursor
  await output();
  await once(rl, "close");
  eventEmitter.cancel();
  return;
}

if (require.main === module) {
  const args: BaseArgs & { watch?: boolean } = baseArgs
    .options({
      watch: {
        alias: "w",
        desc: "Watch and update on changes to the database",
        type: "boolean",
      },
    })
    .parseSync(process.argv.slice(2));
  if (args.watch) {
    chorelistwatch(args).catch((error) => {
      console.error(error);
      process.exit(1);
    });
  } else {
    chorelist(args)
      .catch((error) => {
        console.error(error);
        process.exit(1);
      })
      .then((table) => {
        console.log(table);
      });
  }
}
