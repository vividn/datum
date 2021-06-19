import {
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  jest,
} from "@jest/globals";

import { fail } from "./test-utils";

describe("addDoc", () => {
  it("it adds dataOnly payloads to the given database with the given id", async () => {
    fail();
  });

  it("throws error if trying to add dataOnly payload without id", async () => {
    fail();
  });

  it("adds a datum payload to the database with its calculated id", async () => {
    fail();
  });

  it("throws an error if calculated id does not match a given id for a datum payload", async () => {
    fail();
  });

  it("adds createTime and modifyTime to metadata of datumPayload", async () => {
    fail();
  });

  it("throws error if document with id already exists", async () => {
    fail();
  });

  it("calls another document control method if id already exists and conflict strategy is given", async () => {
    fail();
  });
});