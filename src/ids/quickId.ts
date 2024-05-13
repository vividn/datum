import { EitherPayload } from "../documentControl/DatumDocument";
import { isCouchDbError, MyError } from "../errors";
import { viewMap } from "../views/viewMap";
import { humanIdView, idToHumanView } from "../views/datumViews";
import { startsWith } from "../utils/startsWith";
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

export const ON_AMBIGUOUS_QUICK_ID = [
  "fail",
  "all",
  "first",
  "last",
  "ask",
] as const;

async function exactId(
  db: PouchDB.Database<EitherPayload>,
  quickString: string,
): Promise<string | undefined> {
  try {
    return (await db.get(quickString))._id;
  } catch (error) {
    if (isCouchDbError(error) && error.name === "not_found") {
      //pass
    } else {
      throw error;
    }
  }
  return undefined;
}

async function startsHumanId(
  db: PouchDB.Database<EitherPayload>,
  quickString: string,
  onAmbiguous?: (typeof ON_AMBIGUOUS_QUICK_ID)[number],
): Promise<string[]> {
  const matches = (
    await viewMap({
      db,
      datumView: humanIdView,
      params: startsWith(quickString),
    })
  ).rows;
  if (matches.length > 1) {
    switch (onAmbiguous) {
      case "all":
        return matches.map((row) => row.id);

      case "first": {
        const lowestId = matches.reduce(
          (lowest, row) => (row.id < lowest ? row.id : lowest),
          matches[0].id,
        );
        return [lowestId];
      }

      case "last": {
        const highestId = matches.reduce(
          (highest, row) => (row.id > highest ? row.id : highest),
          matches[0].id,
        );
        return [highestId];
      }

      // case "ask":
      //   throw new NotImplementedError("ask for id");

      default:
        throw new AmbiguousQuickIdError(
          quickString,
          matches.map((row) => row.key),
          matches.map((row) => row.id),
        );
    }
  }
  if (matches.length === 1) {
    return [matches[0].id];
  }
  return [];
}

async function startsMainId(
  db: PouchDB.Database<EitherPayload>,
  quickString: string,
  onAmbiguous?: (typeof ON_AMBIGUOUS_QUICK_ID)[number],
): Promise<string[]> {
  const matches = (await db.allDocs(startsWith(quickString))).rows;
  if (matches.length > 1) {
    switch (onAmbiguous) {
      case "all":
        return matches.map((row) => row.id);

      case "first":
        return [matches[0].id];

      case "last":
        return [matches[matches.length - 1].id];

      // case "ask":
      //   throw new NotImplementedError("ask for id");

      default:
        throw new AmbiguousQuickIdError(
          quickString,
          matches.map((row) => row.id),
          matches.map((row) => row.id),
        );
    }
  }
  if (matches.length === 1) {
    return [matches[0].id];
  }
  return [];
}

export async function quickId(
  db: PouchDB.Database<EitherPayload>,
  quickString: string | string[],
): Promise<string> {
  if (typeof quickString === "string") {
    
  }
  try {
    const doc = await db.get(quickString);
    return doc._id;
  } catch (error) {
    if (isCouchDbError(error) && error.name === "not_found") {
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
      startsHumanId.rows.map((row) => minHumanId(db, row.key)),
    );
    const possibleIds = startsHumanId.rows.map((row) => row.id);
    throw new AmbiguousQuickIdError(quickString, possibleQuickIds, possibleIds);
  }

  const startsMainId = await db.allDocs(startsWith(quickString));
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
      correspondingHumanIds.map((humanId) => minHumanId(db, humanId)),
    );
    throw new AmbiguousQuickIdError(quickString, possibleQuickIds, possibleIds);
  }

  throw new NoQuickIdMatchError(quickString);
}

export async function quickIds(
  db: PouchDB.Database<EitherPayload>,
  quickString: string | string[],
): Promise<string[]> {
  if (Array.isArray(quickString)) {
    return Promise.all(quickString.map((str) => quickId(db, str)));
  }

  const quickStrings = /^,/.test(quickString)
    ? quickString.slice(1).split(",")
    : /,$/.test(quickString)
      ? quickString.slice(0, -1).split(",")
      : /^\[.*]$/.test(quickString)
        ? quickString.slice(1, -1).split(",")
        : [quickString];

  return Promise.all(quickStrings.map((str) => quickId(db, str)));
}
