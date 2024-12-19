import readline from "node:readline/promises";
import { MigrationMapRow, migrationName } from "./migrations";
import { getCmd } from "../commands/getCmd";
import stringify from "string.ify";
import chalk from "chalk";
import { jClone } from "../utils/jClone";
import { OutputArgs, Show } from "../input/outputArgs";
import { migrateOne } from "./migrateOne";
import { once } from "node:events";

export async function interactiveMigration({
  name,
  db,
  outputArgs: originalOutputArgs = {},
}: {
  name: string;
  db: PouchDB.Database;
  outputArgs: OutputArgs;
}): Promise<void> {
  const outputArgs = jClone(originalOutputArgs);
  const showToggle =
    outputArgs.show === Show.All ? Show.Standard : outputArgs.show;
  const migrationRows = (await db.query(migrationName(name), { reduce: false }))
    .rows as MigrationMapRow[];
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });

  while (migrationRows.length > 0) {
    console.clear();
    const row = migrationRows[0];
    const {
      value: { op, data },
    } = row;
    console.info("----------------------");
    await getCmd([row.id], outputArgs);
    console.info(chalk.cyan(`~~~~ OP: ${op} ~~~~`));
    console.info(stringify(data));
    console.info("----------------------");

    const answer = await rl.question(
      "Migrate? [y]es, [n]o, [q]uit, toggle [s]how all: ",
    );
    if (answer === "q") {
      break;
    } else if (answer === "s") {
      outputArgs.show = outputArgs.show === Show.All ? showToggle : Show.All;
    } else if (answer === "y") {
      await migrateOne({ row, db, outputArgs: outputArgs });
      migrationRows.shift();
      await once(rl, "line");
    } else if (answer === "n") {
      migrationRows.shift();
    } else {
      console.error("Invalid input");
    }
  }

  // Your migration code here
  console.info("Migration completed");
  rl.close();
}
