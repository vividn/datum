import { EitherPayload } from "../documentControl/DatumDocument";
import { DatumViewMissingError, isCouchDbError, MyError } from "../errors";

function startingSlices(str: string): string[] {
  let i = str.length;
  const retVal = [];
  while (i--) {
    retVal.unshift(str.slice(0, i + 1));
  }
  return retVal;
}

export async function minHumanId(
  db: PouchDB.Database<EitherPayload>,
  humanId: string
): Promise<string> {
  //TODO: Create function that takes the DatumView object and does view info on it
  const docCountsPerSlice = (
    await db
      .query("datum_sub_human_id", {
        group: true,
        keys: startingSlices(humanId),
      })
      .catch((error) => {
        if (
          isCouchDbError(error) &&
          ["missing", "deleted", "missing_named_view"].includes(error.reason)
        ) {
          throw new DatumViewMissingError();
        }
        throw error;
      })
  ).rows;
  const minWithoutConflict = docCountsPerSlice.find(
    (row) => row.value === 1
  )?.key;
  if (minWithoutConflict === undefined) {
    throw new MinHumanIdError("No substring uniquely identifies document");
  }
  return minWithoutConflict;
}

export class MinHumanIdError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, MinHumanIdError.prototype);
  }
}
