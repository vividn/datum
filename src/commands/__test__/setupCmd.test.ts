import { pass, resetTestDb } from "../../test-utils";
import * as setupDatumViews from "../../views/setupDatumViews";
import { setupCmd } from "../setupCmd";
import * as connectDb from "../../auth/connectDb";
import { EitherPayload } from "../../documentControl/DatumDocument";

describe("setupCmd", () => {
  const dbName = "setup_cmd_test";
  let db: PouchDB.Database<EitherPayload>;

  beforeEach(async () => {
    db = await resetTestDb(dbName);
  });

  afterEach(async () => {
    await db.destroy().catch(pass);
  });

  it("calls connectDb with createDb true by default", async () => {
    jest
      .spyOn(setupDatumViews, "setupDatumViews")
      .mockImplementation(async () => {
        return;
      });
    const connectDbSpy = jest.spyOn(connectDb, "connectDb");
    await setupCmd({ db: dbName });
    expect(connectDbSpy).toHaveBeenCalledWith(
      expect.objectContaining({ createDb: true })
    );
    connectDbSpy.mockReset();

    await setupCmd({ db: dbName, createDb: false });
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
    await setupCmd({ db: dbName });
    expect(setupDatumViewsSpy).toHaveBeenCalledTimes(1);
  });

  it("adds datum views into the db", async () => {
    await setupCmd({ db: dbName });
    await db.get("_design/datum_sub_human_id");
  });

  it("adds project views into the db", async () => {
    await setupCmd({ db: dbName, projectDir: __dirname });
    await db.get("_design/key_value_view");
  });
});
