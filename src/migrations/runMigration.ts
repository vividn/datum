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
    const nextKeyIndex = allMigrationRows.findIndex((row) => row.key !== key);
    const rows = allMigrationRows.splice(
      0,
      nextKeyIndex === -1 ? allMigrationRows.length : nextKeyIndex,
    );

    await Promise.all(
      rows.map(async (row) => {
        await migrateOne({ row, db, outputArgs });
      }),
    );
  }
}
