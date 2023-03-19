import { pass, resetTestDb } from "../../test-utils";
import * as insertDatumViewModule from "../insertDatumView";
import { setupDatumViews } from "../setupDatumViews";
import { _emit } from "../emit";
import * as getAllDatumViews from "../getAllDatumViews";
import { DatumView } from "../DatumView";
import { EitherPayload } from "../../documentControl/DatumDocument";

describe("setupDatumViews", () => {
  const dbName = "setup_datum_views_test";
  let db: PouchDB.Database<EitherPayload>;

  beforeEach(async () => {
    db = await resetTestDb(dbName);
  });

  afterEach(async () => {
    await db.destroy().catch(pass);
  });

  function emit(key: any, value: any) {
    _emit(key, value);
  }

  afterEach(async () => {
    jest.resetModules();
  });

  it("adds all datum views and db views to an empty db", async () => {
    const datumView1: DatumView = {
      name: "datum_view",
      emit,
      map: (doc: any) => {
        emit(doc._id, 1);
      },
      reduce: {
        default: "_count",
      },
    };
    const datumView2: DatumView = {
      name: "datum_another_view",
      emit,
      map: (doc: any) => {
        emit(doc._rev, null);
      },
    };

    const mockAllDatumViews = [datumView1, datumView2];
    jest
      .spyOn(getAllDatumViews, "getAllDatumViews")
      .mockReturnValue(mockAllDatumViews);

    const dbView1: DatumView = {
      name: "project_view",
      emit,
      map: (doc: any) => {
        emit(doc._id, 3);
      },
    };
    const mockDbDatumViews = [dbView1];
    jest
      .spyOn(getAllDatumViews, "getDbDatumViews")
      .mockResolvedValue(mockDbDatumViews);

    const insertDatumViewsSpy = jest.spyOn(
      insertDatumViewModule,
      "insertDatumView"
    );

    await setupDatumViews({ db });

    await db.get("_design/datum_view");
    await db.get("_design/datum_another_view");
    await expect(
      db.get("_design/project_view")
    ).rejects.toThrowErrorMatchingInlineSnapshot(`"missing"`);
    expect(insertDatumViewsSpy).toHaveBeenCalledTimes(2);

    await setupDatumViews({ db, projectDir: "./" });
    await db.get("_design/project_view");
  });
});
