export class MyError extends Error {
  constructor(m: unknown) {
    if (typeof m === "string") {
      super(m);
    } else {
      super();
    }
    this.name = this.constructor.name;
    Object.setPrototypeOf(this, MyError.prototype);
  }
}

export class MissingRequiredKeyError extends MyError {
  constructor(key: string) {
    super(`No data given for the required key: ${key}`);
    Object.setPrototypeOf(this, MissingRequiredKeyError.prototype);
  }
}

export class ExtraDataError extends MyError {
  constructor(keys: string[]) {
    super(
      `the following data do not have keys: ${keys.join(
        ", ",
      )}. Assign keys with equals signs, use required/optional keys, specify a key to use as --remainder, or use --lenient`,
    );
    Object.setPrototypeOf(this, ExtraDataError.prototype);
  }
}
export class BaseDataError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, BaseDataError.prototype);
  }
}

export class IdError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, IdError.prototype);
  }
}

export class BadTimeError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, BadTimeError.prototype);
  }
}

export class BadDateError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, BadDateError.prototype);
  }
}

export class BadDurationError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, BadDurationError.prototype);
  }
}

export class BadTimezoneError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, BadTimezoneError.prototype);
  }
}

export class BadStateError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, BadStateError.prototype);
  }
}

export class MigrationError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, MigrationError.prototype);
  }
}

export class MergeError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, MergeError.prototype);
  }
}

export type CouchDbError = {
  error: string;
  reason: string;
  status: number;
  name: string;
  message: string;
};

export const isCouchDbError = (error: unknown): error is CouchDbError => {
  return !!(error as CouchDbError).error && !!(error as CouchDbError).reason;
};

export class DatumViewMissingError extends MyError {
  constructor(map_name?: unknown) {
    super(
      `Missing internal datum view ${map_name}. Please run setup on this database`,
    );
    Object.setPrototypeOf(this, DatumViewMissingError.prototype);
  }
}
