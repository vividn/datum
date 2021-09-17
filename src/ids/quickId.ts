import { DocumentScope } from "nano";
import { EitherPayload } from "../documentControl/DatumDocument";
import { isCouchDbError, MyError } from "../errors";

export class AmbiguousQuickIdError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, AmbiguousQuickIdError.prototype);
  }
}

export class NoQuickIdMatchError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, NoQuickIdMatchError.prototype);
  }
}

async function quickId(
  db: DocumentScope<EitherPayload>,
  quickString: string
): string {
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
}

export default quickId;
