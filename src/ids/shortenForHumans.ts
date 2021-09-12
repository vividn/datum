import { DocumentScope } from "nano";
import { EitherPayload } from "../documentControl/DatumDocument";
import getHumanIds from "./getHumanIds";
import { minHumanId } from "./minHumanId";

// TODO: make optional n parameter that skips the minHumanId check and just truncates for speed
// it will be interesting to compare how quickly that goes
async function shortenForHumans(
  db: DocumentScope<EitherPayload>,
  ids: string[]
): Promise<(string | undefined)[]> {
  const humanIds = await getHumanIds(db, ids);
  const shortenedHumanIds = Promise.all(
    humanIds.map(async (humanId) => {
      if (humanId === undefined) {
        return undefined;
      }
      return await minHumanId(db, humanId);
    })
  );
  return shortenedHumanIds;
}

export default shortenForHumans;
