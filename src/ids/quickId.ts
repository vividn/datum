import { DocumentScope } from "nano";
import { EitherPayload } from "../documentControl/DatumDocument";
import { isCouchDbError, MyError } from "../errors";
import viewMap from "../views/viewMap";
import { humanIdView, idToHumanView } from "../views/datumViews";
import startsWith from "../utils/startsWith";
import { minHumanId } from "./minHumanId";

export class AmbiguousQuickIdError extends MyError {
  constructor(quickString: string, quickIds: string[], ids: string[]) {
    const idPairs = ids.map((id, index) => `${quickIds[index]}\t${id}`);
    const errorMessage = [
      `${quickString} is ambiguous and may refer to`,
      "quickId\tid",
      ...idPairs,
    ].join("\n");
    super(errorMessage);
    Object.setPrototypeOf(this, AmbiguousQuickIdError.prototype);
  }
}

export class NoQuickIdMatchError extends MyError {
  constructor(quickId: unknown) {
    super(`${quickId} does not match the humanId or id of any documents`);
    Object.setPrototypeOf(this, NoQuickIdMatchError.prototype);
  }
}

async function quickId(
  db: DocumentScope<EitherPayload>,
  quickString: string
): Promise<string> {
  try {
    const doc = await db.get(quickString);
    return doc._id;
  } catch (error) {
    if (isCouchDbError(error) && error.error === "not_found") {
      //pass
    } else {
      throw error;
    }
  }

  const startsHumanId = await viewMap({
    db,
    datumView: humanIdView,
    params: startsWith(quickString),
  });
  if (startsHumanId.rows.length === 1) {
    return startsHumanId.rows[0].id;
  }
  if (startsHumanId.rows.length > 1) {
    const possibleQuickIds = await Promise.all(
      startsHumanId.rows.map((row) => minHumanId(db, row.key))
    );
    const possibleIds = startsHumanId.rows.map((row) => row.id);
    throw new AmbiguousQuickIdError(quickString, possibleQuickIds, possibleIds);
  }

  const startsMainId = await db.list(startsWith(quickString));
  if (startsMainId.rows.length === 1) {
    return startsMainId.rows[0].id;
  }
  if (startsMainId.rows.length > 1) {
    const possibleIds = startsMainId.rows.map((row) => row.id);
    const correspondingHumanIds = (
      await viewMap({
        db,
        datumView: idToHumanView,
        params: { keys: possibleIds },
      })
    ).rows.map((row) => row.value);
    const possibleQuickIds = await Promise.all(
      correspondingHumanIds.map((humanId) => minHumanId(db, humanId))
    );
    throw new AmbiguousQuickIdError(quickString, possibleQuickIds, possibleIds);
  }

  throw new NoQuickIdMatchError(quickString);
}

export default quickId;
