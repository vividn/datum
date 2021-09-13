import { DocumentScope } from "nano";
import { EitherPayload } from "../documentControl/DatumDocument";
import { MyError } from "../errors";

export class AmbiguousQuickIdError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, AmbiguousQuickIdError.prototype);
  }
}

async function quickId(db: DocumentScope<EitherPayload>, quickString: string): string {

}

export default quickId;