import { reduceCmd } from "../../../src/commands/reduceCmd";
import { choreView } from "../views";
import { Show } from "../../../src/input/outputArgs";
import { DateTime } from "luxon";
import chalk from "chalk";
import { ReduceRow } from "../../../src/views/DatumView";
import Table from "easy-table";

async function chorelist() {
  const choresResponse = await reduceCmd({
    mapName: choreView.name,
    groupLevel: 1,
    show: Show.None,
  });
  const choreRows = choresResponse.rows as ReduceRow<typeof choreView>[];
  const chores = choreRows
    .filter((chore) => chore.value.next !== undefined)
    .sort((a, b) => a.value.iti ?? 9e99 - (b.value.iti ?? 9e99));
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
    if (!due) {
      totalDue += 1;
    }
    if (!due && !doneToday) {
      return;
    }
    const iti =
      row.value.iti === undefined ? undefined : Math.floor(row.value.iti);
    const daysOverdue = Math.floor(
      DateTime.fromISO(now).diff(DateTime.fromISO(next), "days").days,
    );
    const color =
      iti === undefined
        ? chalk.white
        : daysOverdue > iti
          ? chalk.red
          : daysOverdue > iti / 2
            ? chalk.yellow
            : chalk.white;

    t.cell("Chore", color(row.key));
    t.cell("ITI", color(iti));
    t.cell("Due", color(next));
    t.cell("Last Done", color(last));
    t.newRow();
  });
  console.log(t.toString());
  if (totalDue === 0) {
    console.log(chalk.green("COMPLETED!\tGOOD WORK!\t\\(◦'⌣'◦)/"));
  }
}

if (require.main === module) {
  chorelist().catch((error) => {
    console.error(error);
    process.exit(1);
  });
}
