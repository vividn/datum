import { insertDatumView } from "./insertDatumView";
import { BaseDocControlArgs } from "../documentControl/base";
import { getAllDatumViews, getDbDatumViews } from "./getAllDatumViews";

export async function setupDatumViews({
  db,
  show,
}: BaseDocControlArgs): Promise<void> {
  const allDatumViews = getAllDatumViews().concat(
    await getDbDatumViews(db.config.db)
  );
  const promises = allDatumViews.map((datumView) => {
    return insertDatumView({ datumView, db, show });
  });
  await Promise.all(promises);
  return;
}
