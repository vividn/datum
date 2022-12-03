import { EitherDocument, EitherPayload } from "./DatumDocument";
import { DocumentScope } from "nano";
import { Show } from "../output/output";

export class DocExistsError extends Error {
  data: { existingDocument: EitherDocument; failedPayload: EitherPayload };
  constructor(failedPayload: EitherPayload, existingDocument: EitherDocument) {
    super(
      `conflict: another doc with id "${existingDocument._id}" already exists`
    );
    this.data = { existingDocument, failedPayload };
    this.name = this.constructor.name;
    Object.setPrototypeOf(this, DocExistsError.prototype);
    Error.captureStackTrace(this, this.constructor);
  }
}

export type BaseDocControlArgs = {
  db: DocumentScope<any>;
  show?: Show;
  formatString?: string;
  KEEP ADDING MORE INFRASTRUCTURE TO FORMATSTRING
};
