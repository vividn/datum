import { insertDatumView } from "./insertDatumView";
import { BaseDocControlArgs } from "../documentControl/base";
import { getAllDatumViews, getDbDatumViews } from "./getAllDatumViews";

export type SetupDatumViewsType = {
  projectDir: string;
} & BaseDocControlArgs

export async function setupDatumViews({
  projectDir,
  db,
  show,
}: SetupDatumViewsType): Promise<void> {
  const allDatumViews = getAllDatumViews();
  const allDbViews = await getDbDatumViews({dbName: db.config.db, projectDir});
  const promises = allDatumViews.concat(allDbViews).map((datumView) => {
    return insertDatumView({ datumView, db, show });
  });
  await Promise.all(promises);
  return;
}
