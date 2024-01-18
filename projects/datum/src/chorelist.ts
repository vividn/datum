import { reduceCmd } from "../../../src/commands/reduceCmd";
import { choreView } from "../views";
import { Show } from "../../../src/input/outputArgs";
import { DateTime } from "luxon";
import chalk from "chalk";

async function chorelist() {
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
  chorelist().catch((error) => {
    console.error(error);
    process.exit(1);
  });
}