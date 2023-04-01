import { EitherPayload } from "../documentControl/DatumDocument";
import { idToHumanView } from "../views/datumViews";
import { DatumViewMissingError, isCouchDbError } from "../errors";

export async function getHumanIds(
  db: PouchDB.Database<EitherPayload>,
  ids: string[]
): Promise<(string | undefined)[]> {
  let viewResponse;
  try {
    viewResponse = await db.query<string>(`${idToHumanView.name}/default`, {
      keys: ids,
      reduce: false,
    });
  } catch (error) {
    if (
      isCouchDbError(error) &&
      ["missing", "deleted", "missing_named_view"].includes(error.reason)
    ) {
      throw new DatumViewMissingError();
    }
    throw error;
  }
  const existingIdToHuman = viewResponse.rows;
  const returnValue = ids.map((id) => {
    if (existingIdToHuman.length === 0 || existingIdToHuman[0].key !== id) {
      return undefined;
    }
    return existingIdToHuman.shift()!.value;
  });
  return returnValue;
}
