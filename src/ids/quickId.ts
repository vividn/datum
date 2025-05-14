import { EitherPayload } from "../documentControl/DatumDocument";
import { isCouchDbError, LastDocsTooOldError, MyError } from "../errors";
import { viewMap } from "../views/viewMap";
import { humanIdView, idToHumanView } from "../views/datumViews";
import { startsWith } from "../utils/startsWith";
import { splitCommaString } from "../utils/splitCommaString";
import { minHumanId } from "./minHumanId";
import { JsonType, QueryOptions } from "../utils/utilityTypes";
import { getLastDocs } from "../documentControl/lastDocs";
import { DateTime } from "luxon";
import { MainDatumArgs } from "../input/mainArgs";
import { QuickIdArgs } from "../input/quickIdArg";
import { connectDb } from "../auth/connectDb";
import { timingView } from "../views/datumViews/timingView";
import { reverseViewParams } from "../utils/reverseViewParams";
import { HIGH_STRING } from "../utils/startsWith";

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
    super(
      `${quickId} does not match any documents. Try using '_' for the most recent document.`,
    );
    Object.setPrototypeOf(this, NoQuickIdMatchError.prototype);
  }
}

export const ON_AMBIGUOUS_QUICK_ID = [
  "fail",
  "all",
  "first",
  "last",
  // TODO: "ask",
] as const;

export const _LAST_WITH_PROTECTION = "_LAST_WITH_PROTECTION";
export const _LAST = "_LAST";

export const _RECENT = "_";
export const _RECENT_REGEX = /^(_+)(?:(\d+):)?([a-zA-Z0-9]*)$/;

async function specialQuickId(
  quickString: string,
  db: PouchDB.Database<EitherPayload>,
): Promise<string[]> {
  if ([_LAST, _LAST_WITH_PROTECTION].includes(quickString)) {
    const lastDocsRef = await getLastDocs(db);
    const refTime = DateTime.fromISO(lastDocsRef.time);
    if (
      DateTime.utc().diff(refTime, "minutes").minutes > 15 &&
      quickString === _LAST_WITH_PROTECTION
    ) {
      throw new LastDocsTooOldError(
        "Last docs changed more than 15 minutes ago. To explicitly use these documents, either `get` them again first or use '_LAST' as the quick id",
      );
    }
    return lastDocsRef.ids;
  }
  const match = quickString.match(_RECENT_REGEX);
  if (match) {
    const [, underscores, numberStr, fieldName] = match;
    let position = underscores.length;

    if (numberStr) {
      position = parseInt(numberStr, 10);
      return getRecentDocumentByPosition(db, position, fieldName || null);
    }

    if (underscores.length === 1 && /^\d+$/.test(fieldName)) {
      position = parseInt(fieldName, 10);
      return getRecentDocumentByPosition(db, position, null);
    }

    return getRecentDocumentByPosition(db, position, fieldName || null);
  }

  return [];
}

async function exactId(
  quickString: string,
  db: PouchDB.Database<EitherPayload>,
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
  quickString: string,
  db: PouchDB.Database<EitherPayload>,
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

      default: {
        const possibleQuickIds = await Promise.all(
          matches.map((row) => minHumanId(db, row.key)),
        );
        const possibleIds = matches.map((row) => row.id);
        throw new AmbiguousQuickIdError(
          quickString,
          possibleQuickIds,
          possibleIds,
        );
      }
    }
  }
  if (matches.length === 1) {
    return [matches[0].id];
  }
  return [];
}

async function startsMainId(
  quickString: string,
  db: PouchDB.Database<EitherPayload>,
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

      default: {
        const possibleIds = matches.map((row) => row.id);
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
        throw new AmbiguousQuickIdError(
          quickString,
          possibleQuickIds,
          possibleIds,
        );
      }
    }
  }
  if (matches.length === 1) {
    return [matches[0].id];
  }
  return [];
}

async function searchForQuickId(
  quickString: string,
  db: PouchDB.Database<EitherPayload>,
  onAmbiguous?: (typeof ON_AMBIGUOUS_QUICK_ID)[number],
): Promise<string[]> {
  const special = await specialQuickId(quickString, db);
  if (special.length > 0) {
    return special;
  }
  const exact = await exactId(quickString, db);
  if (exact) {
    return [exact];
  }
  const matchesHumanId = await startsHumanId(quickString, db, onAmbiguous);
  if (matchesHumanId.length > 0) {
    return matchesHumanId;
  }
  const matchesMainId = await startsMainId(quickString, db, onAmbiguous);
  if (matchesMainId.length > 0) {
    return matchesMainId;
  }
  throw new NoQuickIdMatchError(quickString);
}

export async function quickId(
  quickValue: string | string[] | JsonType,
  args: MainDatumArgs & QuickIdArgs,
): Promise<string[]> {
  const db = connectDb(args);

  let quickArray: string[];
  if (Array.isArray(quickValue)) {
    quickArray = quickValue.map(String);
  } else {
    const maybeSplit = splitCommaString(String(quickValue));
    if (Array.isArray(maybeSplit)) {
      // try for the speical case where the id has commas in it and no splitting is wanted
      try {
        const matchedWithComma = await searchForQuickId(
          String(quickValue),
          db,
          args.onAmbiguousQuickId,
        );
        return matchedWithComma;
      } catch (error) {
        if (error instanceof NoQuickIdMatchError) {
          // pass
        } else {
          throw error;
        }
      }
      quickArray = maybeSplit;
    } else {
      quickArray = [maybeSplit];
    }
  }

  const idPromises = quickArray.map(async (str) =>
    searchForQuickId(str, db, args.onAmbiguousQuickId),
  );

  return (await Promise.all(idPromises)).flat();
}

async function getRecentDocumentByPosition(
  db: PouchDB.Database<EitherPayload>,
  position: number,
  field: string | null,
): Promise<string[]> {
  const viewParams: QueryOptions = {
    include_docs: true,
    inclusive_end: true,
    limit: position,
    startkey: ["hybrid", field, ""],
    endkey: ["hybrid", field, HIGH_STRING],
  };

  const reversedParams = reverseViewParams(viewParams);

  const viewResults = await viewMap({
    db,
    datumView: timingView,
    params: reversedParams,
  });

  const rows = viewResults.rows;
  if (rows.length < position) {
    throw new NoQuickIdMatchError(
      `Not enough documents to get position ${position}`,
    );
  }

  const doc = rows[position - 1].doc;
  if (!doc) {
    throw new NoQuickIdMatchError(`Document at position ${position} not found`);
  }

  return [doc._id];
}
