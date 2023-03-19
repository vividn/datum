import { insertDatumView } from "./insertDatumView";
import { BaseDocControlArgs } from "../documentControl/base";
import { getAllDatumViews, getDbDatumViews } from "./getAllDatumViews";

export type SetupDatumViewsType = {
  projectDir?: string;
} & BaseDocControlArgs;

export async function setupDatumViews({
  projectDir,
  db,
  outputArgs,
}: SetupDatumViewsType): Promise<void> {
  const allDatumViews = getAllDatumViews();
  const dbName = db.name.split("/").at(-1) as string;
  const allDbViews = projectDir
    ? await getDbDatumViews({ dbName, projectDir })
    : [];
  const promises = allDatumViews.concat(allDbViews).map((datumView) => {
    return insertDatumView({ datumView, db, outputArgs: outputArgs });
  });
  await Promise.all(promises);
  return;
}
