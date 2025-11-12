import { insertDatumView } from "./insertDatumView.js";
import { BaseDocControlArgs } from "../documentControl/base.js";
import { getAllDatumViews, getDbDatumViews } from "./getAllDatumViews.js";

export type SetupDatumViewsType = {
  projectDir?: string;
} & BaseDocControlArgs;

export async function setupDatumViews({
  projectDir,
  db,
  outputArgs,
}: SetupDatumViewsType): Promise<void> {
  const coreDatumViews = getAllDatumViews();
  const dbName = db.name.split("/").at(-1) as string;
  const dbViews = projectDir
    ? await getDbDatumViews({ dbName, projectDir })
    : [];
  const dbMigrations = projectDir
    ? await getDbDatumViews({ dbName, projectDir, subfolder: "migrations" })
    : [];
  const promises = coreDatumViews
    .concat(dbViews)
    .concat(dbMigrations)
    .map((datumView) => {
      return insertDatumView({ datumView, db, outputArgs: outputArgs });
    });
  await Promise.all(promises);
  return;
}
