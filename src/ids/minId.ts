import { DocumentScope } from "nano";
import { EitherPayload } from "../documentControl/DatumDocument";

function startingSlices(str: string): string[] {
  let i = str.length;
  const retVal = [];
  while (i--) {
    retVal.unshift(str.slice(0, i + 1));
  }
  return retVal;
}

export async function minId(
  db: DocumentScope<EitherPayload>,
  humanId: string
): Promise<string> {
  //TODO: Create function that takes the DatumView object and does view info on it
  const docCountsPerSlice = (
    await db.view("datum_sub_human_id", "datum_sub_human_id", {
      group: true,
      keys: startingSlices(humanId),
    })
  ).rows;
  const minWithoutConflict = docCountsPerSlice.find(
    (row) => row.value === 1
  )?.key;
  if (minWithoutConflict === undefined) {
    throw Error("No substring uniquely identifies document");
  }
  return minWithoutConflict;
}