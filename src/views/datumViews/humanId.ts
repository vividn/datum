import { EitherDocument, EitherPayload } from "../../documentControl/DatumDocument";
import { DatumView } from "../getViewDoc";
import emit from "../emit";
import { DocumentScope } from "nano";

export const humanIdView: DatumView<EitherDocument> = {
  name: "datum_human_id",
  map: (doc) => {
    if (doc.meta && doc.meta.humanId) {
      emit(doc.meta.humanId, null);
    }
  },
};

export const subHumanIdView: DatumView<EitherDocument> = {
  name: "datum_sub_human_id",
  map: (doc) => {
    if (doc.meta && doc.meta.humanId) {
      const hid = doc.meta.humanId;
      let i = hid.length;
      while (i--) {
        emit(hid.slice(0, i + 1), null);
      }
    }
  },
  reduce: "_count",
};

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
