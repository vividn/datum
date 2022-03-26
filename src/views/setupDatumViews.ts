import { insertDatumView } from "./insertDatumView";
import { BaseDocControlArgs } from "../documentControl/base";
import { getAllDatumViews } from "./getAllDatumViews";

export async function setupDatumViews({
  db,
  show,
}: BaseDocControlArgs): Promise<void> {
  const allDatumViews = getAllDatumViews();
  const promises = allDatumViews.map((datumView) => {
    return insertDatumView({ datumView, db, show });
  });
  await Promise.all(promises);
  return;
}
