import {
  it,
  jest,
  expect,
} from "@jest/globals";
import { testDbLifecycle } from "../../test-utils";
import * as setupDatumViews from "../../views/setupDatumViews";
import { setupCmd } from "../setupCmd";

const dbName = "setup_cmd_test";
const db = testDbLifecycle(dbName);

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
