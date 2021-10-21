import {
  it,
  jest,
  beforeEach,
  afterEach,
  beforeAll,
  expect,
} from "@jest/globals";
import { pass, testNano } from "../../../test/test-utils";
import * as setupDatumViews from "../../views/setupDatumViews";
import { setupCmd } from "../setupCmd";

const dbName = "setup_cmd_test";
const db = testNano.use(dbName);

beforeAll(async () => {
  await testNano.db.destroy(dbName).catch(pass);
});

beforeEach(async () => {
  await testNano.db.create(dbName);
});

afterEach(async () => {
  await testNano.db.destroy(dbName);
});

it("calls setupDatumViews", async () => {
  const setupDatumViewsSpy = jest
    .spyOn(setupDatumViews, "default")
    .mockImplementation(async () => {
      return;
    });
  await setupCmd({ db: dbName });
  expect(setupDatumViewsSpy).toHaveBeenCalledTimes(1);
  setupDatumViewsSpy.mockRestore();
});

it("adds datum views into the db", async () => {
  await setupCmd({ db: dbName });
  await db.get("_design/datum_sub_human_id");
});
