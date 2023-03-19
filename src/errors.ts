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

export class DataError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, DataError.prototype);
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
  constructor(map_name?: unknown, reduce_name?: unknown) {
    super(
      `Missing internal datum view ${map_name} ${reduce_name} Please run setup on this database`
    );
    Object.setPrototypeOf(this, DatumViewMissingError.prototype);
  }
}
