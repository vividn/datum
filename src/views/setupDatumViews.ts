import insertDatumView from "./insertDatumView";
import { BaseDocControlArgs } from "../documentControl/base";
import getAllDatumViews from "./getAllDatumViews";

const allDatumViews = getAllDatumViews();

export default async function setupDatumViews({
  db,
  show,
}: BaseDocControlArgs): Promise<void> {
  const promises = allDatumViews.map((datumView) => {
    insertDatumView({ datumView, db, show });
  });
  await Promise.all(promises);
  return;
}
