import { EitherPayload } from "../documentControl/DatumDocument";
import { DocumentScope } from "nano";
import { humanIdView } from "../views/datumViews";

async function getHumanIds(
  db: DocumentScope<EitherPayload>,
  ids: string[]
): Promise<(string | undefined)[]> {
  const existingIdToHuman = (await db.view<string>(humanIdView.name, "default", {keys: ids, reduce: false})).rows;
  const returnValue = ids.map((id) => {
    if (existingIdToHuman[0].key === id) {
      return existingIdToHuman.shift()!.value;
    }
    return undefined;
  });
  return returnValue;
}

export default getHumanIds;
