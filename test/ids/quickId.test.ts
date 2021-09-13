import { afterEach, beforeAll, beforeEach, test, jest } from "@jest/globals";
import { pass, testNano } from "../test-utils";
import insertDatumView from "../../src/views/insertDatumView";
import { idToHumanView, subHumanIdView } from "../../src/views/datumViews";

const dbName = "test_quick_id";
const db = testNano.use(dbName);

beforeAll(async () => {
  await testNano.db.destroy(dbName).catch(pass);
});

beforeEach(async () => {
  await testNano.db.create(dbName);
  await insertDatumView({db, datumView: idToHumanView});
  await insertDatumView({db, datumView: subHumanIdView});
});

afterEach(async () => {
  await testNano.db.destroy(dbName).catch(pass);
  jest.restoreAllMocks();
});

test.todo("it returns the input if the exact id exists in the database");
test.todo("if the text matches the beginning of exactly one humanId, it returns the associated _id");
test.todo("if the text matches more than one humanId, it throws an error, showing the possible matches");
test.todo("if no human ids match, and string matches the beginning of exactly one _id, return that _id");
test.todo("no humanIds match, but several _ids match starting sub string, throw error and show possible matches");
test.todo("if the substring starts both an id and a human id, then it prefers the humanId");
test.todo("if the string matches an id exactly and a human id exactly, prefer the id match");
test.todo("if the string matches an id exactly and is the begining of one or more human ids, prefer the id");
test.todo("if no humanIds or ids match at all, throw an error");
