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

export class BadTimeArgError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, BadTimeArgError.prototype);
  }
}

export class BadDateArgError extends MyError {
  constructor(m: unknown) {
    super(m);
    Object.setPrototypeOf(this, BadDateArgError.prototype);
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
  scope: "couch";
  statusCode: number;
  errid: string;
  description: string;
  error: string;
  reason: string;
};

export const isCouchDbError = (error: unknown): error is CouchDbError => {
  return (error as CouchDbError).scope === "couch";
};

export class DatumViewMissing extends MyError {
  constructor() {
    super("Internal datum view is missing. Please run setup on this database");
    Object.setPrototypeOf(this, DatumViewMissing.prototype);
  }
}
