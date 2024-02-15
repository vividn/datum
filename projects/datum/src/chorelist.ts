#!/usr/bin/env node

import { reduceCmd } from "../../../src/commands/reduceCmd";
import { choreView } from "../views";
import { OutputArgs, outputArgs, Show } from "../../../src/input/outputArgs";
import { DateTime } from "luxon";
import chalk from "chalk";
import { ReduceRow } from "../../../src/views/DatumView";
import Table from "easy-table";
import { connectDb } from "../../../src/auth/connectDb";
import readline from "node:readline";
import { stdin, stdout } from "node:process";
import { once } from "node:events";
import { insertDatumView } from "../../../src/views/insertDatumView";
import { isoDate, isoDatetime } from "../../../src/time/timeUtils";
import { dbArgs, DbArgs } from "../../../src/input/dbArgs";
import { ArgumentParser } from "argparse";
import { parseIfNeeded } from "../../../src/utils/parseIfNeeded";

let oneTimeSetup = false;
const possibleSorts = ["field", "iti", "due", "last"] as const;
type PossibleSorts = (typeof possibleSorts)[number];

const sortFunctions: Record<
  PossibleSorts,
  (a: ReduceRow<typeof choreView>, b: ReduceRow<typeof choreView>) => number
> = {
  field: (a, b) => a.key.localeCompare(b.key),
  iti: (a, b) => (a.value.iti ?? 9e99) - (b.value.iti ?? 9e99),
  due: (a, b) => {
    const aNext = a.value.next ?? "9999-12-31";
    const bNext = b.value.next ?? "9999-12-31";
    return aNext.localeCompare(bNext);
  },
  last: (a, b) => {
    const aLast =
      a.value.lastOccur === "0000-00-00" ? "9999-12-31" : a.value.lastOccur;
    const bLast =
      b.value.lastOccur === "0000-00-00" ? "9999-12-31" : b.value.lastOccur;
    return aLast.localeCompare(bLast);
  },
};

function timeIfTodayElseDate(dateOrTime?: isoDate | isoDatetime): string {
  if (dateOrTime === undefined) {
    return "";
  }
  if (DateTime.fromISO(dateOrTime).isValid === false) {
    return "NEVER";
  }
  if (!dateOrTime.includes("T")) {
    return dateOrTime;
  }
  const datetime = DateTime.fromISO(dateOrTime).toLocal();
  const date = DateTime.fromISO(dateOrTime).toLocal().toISODate() as string;
  return date === DateTime.local().toISODate()
    ? datetime.toLocaleString(DateTime.TIME_24_SIMPLE)
    : date;
}

function isDue(dateOrTime?: isoDate | isoDatetime): boolean {
  if (dateOrTime === undefined) {
    return false;
  }
  if (DateTime.fromISO(dateOrTime).isValid === false) {
    return false;
  }
  if (!dateOrTime.includes("T")) {
    return dateOrTime <= DateTime.local().toISODate();
  }
  return dateOrTime <= DateTime.utc().toISOTime();
}

function isDoneToday(dateOrTime?: isoDate | isoDatetime): boolean {
  if (dateOrTime === undefined) {
    return false;
  }
  if (DateTime.fromISO(dateOrTime).isValid === false) {
    return false;
  }
  if (!dateOrTime.includes("T")) {
    return dateOrTime === DateTime.local().toISODate();
  }
  return (
    DateTime.fromISO(dateOrTime).toLocal().toISODate() ===
    DateTime.local().toISODate()
  );
}

type ChorelistArgs = { watch?: boolean; sort?: PossibleSorts } & DbArgs &
  OutputArgs;

async function chorelist(args: ChorelistArgs): Promise<string> {
  if (!oneTimeSetup) {
    const db = connectDb(args);
    await insertDatumView({
      db,
      datumView: choreView,
      outputArgs: { show: Show.Minimal },
    });
    oneTimeSetup = true;
  }
  const showAll = args.show === Show.All || args.showAll;
  const sortFn = sortFunctions[args.sort as PossibleSorts] ?? sortFunctions.iti;

  const choresResponse = await reduceCmd({
    ...args,
    mapName: choreView.name,
    groupLevel: 1,
    show: Show.None,
  });
  const choreRows = choresResponse.rows as ReduceRow<typeof choreView>[];
  const chores = choreRows
    .filter((chore) => chore.value.next !== undefined)
    .sort(sortFn);

  const today = DateTime.local().toISODate();
  const t = new Table();

  let totalDue = 0;
  chores.forEach((row) => {
    if (row.value.next === undefined) {
      return;
    }
    const due = isDue(row.value.next);
    const doneToday = isDoneToday(row.value.lastOccur);
    if (!due && !doneToday && !showAll) {
      return;
    }
    if (due) {
      totalDue += 1;
    }
    const next = timeIfTodayElseDate(row.value.next);
    const last = timeIfTodayElseDate(row.value.lastOccur);
    const iti =
      row.value.iti === undefined ? undefined : Math.floor(row.value.iti);
    const daysOverdue = Math.floor(
      DateTime.fromISO(today).diff(DateTime.fromISO(next), "days").days,
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

async function chorelistwatch(args: DbArgs): Promise<void> {
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

const chorelistArgs = new ArgumentParser({
  description: "List chores",
  add_help: true,
  prog: "chorelist",
  parents: [dbArgs, outputArgs],
});
chorelistArgs.add_argument("sort", {
  type: "str",
  help: "Sort the output",
  choices: [...possibleSorts],
  default: "iti",
  nargs: "?",
});
chorelistArgs.add_argument("--watch", {
  action: "store_true",
  help: "Watch and update on changes to the database",
});

async function chorelistCmd(argsOrCli: ChorelistArgs | string | string[]) {
  const args = parseIfNeeded(chorelistArgs, argsOrCli);
  if (args.watch) {
    await chorelistwatch(args);
  } else {
    const table = await chorelist(args);
    console.log(table);
  }
}

if (require.main === module) {
  chorelistCmd(process.argv.slice(2)).catch((err) => {
    throw err;
  });
}
