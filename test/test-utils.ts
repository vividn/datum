import Nano from "nano";
import { CouchDbError } from "../src/errors";

// eslint-disable-next-line @typescript-eslint/no-empty-function
export const pass = (): void => {};
export const fail = (): never => {
  throw Error;
};

export const testNano = Nano(`http://admin:password@localhost:5983`);

export const mockDocMissingError: CouchDbError = {
  scope: "couch",
  statusCode: 404,
  errid: "non_200",
  description: "missing",
  error: "not_found",
  reason: "missing",
};

export const mockDocDeletedError: CouchDbError = {
  scope: "couch",
  statusCode: 404,
  errid: "non_200",
  description: "deleted",
  error: "not_found",
  reason: "deleted",
};

export const mockMissingNamedViewError: CouchDbError = {
  scope: "couch",
  statusCode: 404,
  errid: "non_200",
  description: "missing_named_view",
  error: "not_found",
  reason: "missing_named_view",
};

export async function resetTestDb(dbName: string): Promise<void> {
  const maxTries = 3;
  let tries = 0;
  await testNano.db.destroy(dbName).catch(pass);
  while (tries++ <= maxTries) {
    await testNano.db.destroy(dbName).catch(pass);
    await testNano.db.create(dbName).catch(pass);
    if ((await testNano.db.list()).includes(dbName)) {
      return;
    }
  }
  throw Error(`Unable to reset database after ${maxTries} attempts`);
}
