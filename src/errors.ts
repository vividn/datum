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
