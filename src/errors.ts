class DataError extends Error {
  constructor(message: string) {
    super(message);
    this.name = this.constructor.name;
  }
}

class PayloadError extends Error {
  constructor(message: string) {
    super(message);
    this.name = this.constructor.name;
  }
}

class MigrationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = this.constructor.name;
  }
}

module.exports = { DataError, PayloadError, MigrationError };
