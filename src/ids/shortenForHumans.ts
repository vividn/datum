import { DocumentScope } from "nano";
import { EitherPayload } from "../documentControl/DatumDocument";
import getHumanIds from "./getHumanIds";
import { minHumanId } from "./minHumanId";

async function shortenForHumans(
  db: DocumentScope<EitherPayload>,
  ids: string[]
): Promise<(string | undefined)[]> {
  const humanIds = await getHumanIds(db, ids);
  const shortenedHumanIds = Promise.all(humanIds.map(async (humanId) => {
    if (humanId === undefined) {
      return undefined;
    }
    return await minHumanId(db, humanId);
  }));
  return shortenedHumanIds;
}

export default shortenForHumans;
