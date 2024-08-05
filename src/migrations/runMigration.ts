import { OutputArgs } from "../input/outputArgs";
import { migrateOne } from "./migrateOne";
import { migrationName } from "./migrations";

export async function runMigration({
  name,
  db,
  outputArgs = {},
}: {
  name: string;
  db: PouchDB.Database;
  outputArgs: OutputArgs;
}): Promise<void> {
  const allMigrationRows = (
    await db.query(migrationName(name), { reduce: false })
  ).rows;

  // Run all rows with the same key in parallel
  while (allMigrationRows.length) {
    const key = allMigrationRows[0].key;
    const rows = allMigrationRows.splice(
      0,
      allMigrationRows.findIndex((row) => row.key !== key),
    );

    await Promise.all(
      rows.map(async (row) => {
        await migrateOne({ row, db, outputArgs });
      }),
    );
  }
}
