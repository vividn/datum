// This is method is only temporary until better undo infrastructure is developed.
import {
  EitherDocument,
  EitherPayload,
} from "../documentControl/DatumDocument";
import { assembleId } from "../ids/assembleId";
import { isCouchDbError } from "../errors";
import { DateTime, Duration } from "luxon";
import { showDelete } from "../output/output";
import { OutputArgs } from "../input/outputArgs";
import { pullOutData } from "../utils/pullOutData";

export async function primitiveUndo({
  db,
  payload,
  force,
  outputArgs = {},
}: {
  db: PouchDB.Database;
  payload: EitherPayload;
  force?: boolean;
  outputArgs: OutputArgs;
}): Promise<EitherDocument> {
  const _id = payload._id ?? assembleId({ payload });
  let doc: EitherDocument;
  try {
    doc = await db.get(_id);
  } catch (error) {
    // if the id involves a time, then there could be some slight difference in the id
    const idStructure = payload.meta?.idStructure ?? "";
    if (
      isCouchDbError(error) &&
      error.reason === "missing" &&
      idStructure.match(/%\??(create|modify|occur)Time%/)
    ) {
      // just get the next lowest id
      doc = (
        await db.allDocs({
          startkey: _id,
          descending: true,
          limit: 1,
          include_docs: true,
        })
      ).rows[0]?.doc as EitherDocument;
      if (doc === undefined) {
        throw error;
      }
    } else {
      throw error;
    }
  }
  const { data, meta } = pullOutData(doc);

  const fifteenMinutesAgo = DateTime.now().minus(
    Duration.fromObject({ minutes: 15 }),
  );
  if (
    meta?.createTime &&
    DateTime.fromISO(meta.createTime.utc) < fifteenMinutesAgo
  ) {
    if (!force) {
      // deletion prevention
      throw Error("Doc created more than fifteen minutes ago");
    }
    console.log("Doc created more than fifteen minutes ago");
  }
  await db.remove(doc._id, doc._rev);
  showDelete(doc, outputArgs);
  return doc;
}
