import { MyError } from "../errors";
import { isoDatetime } from "../time/timeUtils";

export type StateChangeErrorType = {
  message?: string;
  field: string;
  occurTime: isoDatetime;
  ids: string[];
};

export class StateChangeError extends MyError implements StateChangeErrorType {
  field: string;
  occurTime: isoDatetime;
  ids: string[];

  constructor(args: StateChangeErrorType) {
    super(args.message ?? "State change error");
    this.field = args.field;
    this.occurTime = args.occurTime;
    this.ids = args.ids;

    Object.setPrototypeOf(this, StateChangeError.prototype);
  }
}
