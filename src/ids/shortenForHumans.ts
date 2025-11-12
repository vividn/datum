import { EitherPayload } from "../documentControl/DatumDocument.js";
import { getHumanIds } from "./getHumanIds.js";
import { minHumanId } from "./minHumanId.js";

// TODO: make optional n parameter that skips the minHumanId check and just truncates for speed
// it will be interesting to compare how quickly that goes
export async function shortenForHumans(
  db: PouchDB.Database<EitherPayload>,
  ids: string[],
): Promise<(string | undefined)[]> {
  const humanIds = await getHumanIds(db, ids);
  const shortenedHumanIds = Promise.all(
    humanIds.map(async (humanId) => {
      if (humanId === undefined) {
        return undefined;
      }
      return await minHumanId(db, humanId);
    }),
  );
  return shortenedHumanIds;
}
