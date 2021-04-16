export class DataError extends Error {
  constructor(message: string) {
    super(message);
    this.name = this.constructor.name;
  }
}

export class PayloadError extends Error {
  constructor(message: string) {
    super(message);
    this.name = this.constructor.name;
  }
}
