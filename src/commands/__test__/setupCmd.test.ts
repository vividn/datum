import { testDbLifecycle } from "../../__test__/test-utils";
import * as setupDatumViews from "../../views/setupDatumViews";
import { setupCmd } from "../setupCmd";
import * as connectDb from "../../auth/connectDb";

describe("setupCmd", () => {
  const dbName = "setup_cmd_test";
  const db = testDbLifecycle(dbName);

  it("calls connectDb with createDb true by default", async () => {
    jest
      .spyOn(setupDatumViews, "setupDatumViews")
      .mockImplementation(async () => {
        return;
      });
    const connectDbSpy = jest.spyOn(connectDb, "connectDb");
    await setupCmd("");
    expect(connectDbSpy).toHaveBeenCalledWith(
      expect.objectContaining({ createDb: true })
    );
    connectDbSpy.mockReset();

    await setupCmd("--no-create-db");
    expect(connectDbSpy).toHaveBeenCalledWith(
      expect.objectContaining({ createDb: false })
    );
  });

  it("calls setupDatumViews", async () => {
    const setupDatumViewsSpy = jest
      .spyOn(setupDatumViews, "setupDatumViews")
      .mockImplementation(async () => {
        return;
      });
    await setupCmd("");
    expect(setupDatumViewsSpy).toHaveBeenCalledTimes(1);
  });

  it("adds datum views into the db", async () => {
    await setupCmd("");
    await db.get("_design/datum_sub_human_id");
  });

  it("adds project views into the db", async () => {
    await setupCmd(`--project-dir ${__dirname}`);
    await db.get("_design/key_value_view");
  });
});
