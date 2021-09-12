import { EitherPayload } from "../documentControl/DatumDocument";
import { DocumentScope } from "nano";
import { idToHumanView } from "../views/datumViews";
import { DatumViewMissingError, isCouchDbError } from "../errors";

async function getHumanIds(
  db: DocumentScope<EitherPayload>,
  ids: string[]
): Promise<(string | undefined)[]> {
  let viewResponse;
  try {
    viewResponse = await db.view<string>(idToHumanView.name, "default", {
      keys: ids,
      reduce: false,
    });
  } catch (error) {
    //TODO make special error checking missingDatumView
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

export default getHumanIds;
