export class MyError extends Error {
  constructor(m?: unknown) {
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
  #key: string = "";
  #value: string = "";

  constructor(badTime?: string) {
    super();
    this.#value = badTime ? "";
    Object.setPrototypeOf(this, BadTimeError.prototype);
  }

  get message(): string {
    const withValue = this.#value ? `"${this.#value}" is an ` : "";
    const withKey = this.#key ? `, key: ${this.#key}` : "";
    return `${withValue}invalid time${withKey}`;
  }
  set key(key: string) {
    this.#key = key;
  }
  get key(): string {
    return this.#key;
  }
  get value(): string {
    return this.#value;
  }
}

export class BadDateError extends MyError {
  #key: string = "";
  #value: string = "";

  constructor(badDate?: string) {
    super();
    this.#value = badDate ?? "";
    Object.setPrototypeOf(this, BadDateError.prototype);
  }

  get message(): string {
    const withValue = this.#value ? `"${this.#value}" is an ` : "";
    const withKey = this.#key ? `, key: ${this.#key}` : "";
    return `${withValue}invalid date${withKey}`;
  }
  set key(key: string) {
    this.#key = key;
  }
  get key(): string {
    return this.#key;
  }
  get value(): string {
    return this.#value;
  }
}

export class BadDurationError extends MyError {
  #key: string = "";
  #value: string = "";

  constructor(badDate?: string) {
    super();
    this.#value = badDate ?? "";
    Object.setPrototypeOf(this, BadDurationError.prototype);
  }

  get message(): string {
    const withValue = this.#value ? `"${this.#value}" is an ` : "";
    const withKey = this.#key ? `, key: ${this.#key}` : "";
    return `${withValue}invalid duration${withKey}`;
  }
  set key(key: string) {
    this.#key = key;
  }
  get key(): string {
    return this.#key;
  }
  get value(): string {
    return this.#value;
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
      `Missing internal datum view ${map_name} ${reduce_name} Please run setup on this database`,
    );
    Object.setPrototypeOf(this, DatumViewMissingError.prototype);
  }
}
