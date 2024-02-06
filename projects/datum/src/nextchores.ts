#!/usr/bin/env node

import { choreView } from "../views";
import { reduceCmd } from "../../../src/commands/reduceCmd";
import { DateTime } from "luxon";
import { Show } from "../../../src/input/outputArgs";
import { ReduceRow } from "../../../src/views/DatumView";
import { baseArgs, BaseArgs } from "../../../src/input/baseArgs";
import { connectDb } from "../../../src/auth/connectDb";
import { insertDatumView } from "../../../src/views/insertDatumView";
import readline from "node:readline";
import { stdin, stdout } from "node:process";
import { once } from "node:events";

function choreRowToLastNextChore(row: ReduceRow<typeof choreView>): string {
  const { key, value } = row;
  const last =
    value.lastOccur === "0000-00-00"
      ? "##NEVER## "
      : value.lastOccur.slice(0, 10);
  const next = value.next ? value.next.slice(0, 10) : "INACTIVE";
  return [last, next, key].join("\t");
}

let oneTimeSetup = false;
async function nextchores(args: BaseArgs): Promise<string> {
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
  const chores = choresResponse.rows
    .filter((chore) => chore.value.next !== undefined)
    .sort(
      (a, b) =>
        new Date(a.value.next).valueOf() - new Date(b.value.next).valueOf(),
    );
  const now = DateTime.local().toISODate();
  const due = chores.filter((row) => row.value.next <= now);
  const future = chores.filter((row) => row.value.next > now);
  const tenDash = "----------";
  const header = "Last Done \tNext Date \tChore";
  const dueChores = due.map((row) => choreRowToLastNextChore(row)).join("\n");
  const dueDivider = process.stdout.isTTY
    ? `\n${tenDash}\t${tenDash}\t${tenDash}`
    : "";
  const futureChores = future
    .map((row) => choreRowToLastNextChore(row))
    .join("\n");
  return `${header}\n${dueChores}${dueDivider}\n${futureChores}`;
}

async function nextchoreswatch(args: BaseArgs): Promise<void> {
  const db = connectDb(args);

  async function output(): Promise<void> {
    const choreTable = await nextchores(args);
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
    nextchoreswatch(args).catch((error) => {
      console.error(error);
      process.exit(1);
    });
  } else {
    nextchores(args)
      .catch((error) => {
        console.error(error);
        process.exit(1);
      })
      .then((table) => {
        console.log(table);
      });
  }
}
