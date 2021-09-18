import { pass, resetTestDb, testNano } from "../test-utils";
import { ViewPayload } from "../../src/views/viewDocument";
import { afterEach, beforeEach, expect, it, jest } from "@jest/globals";
import * as insertDatumViewModule from "../../src/views/insertDatumView";
import setupDatumViews from "../../src/views/setupDatumViews";
import _emit from "../../src/views/emit";
import * as getAllDatumViews from "../../src/views/getAllDatumViews";

const dbName = "setup_datum_views_test";
const db = testNano.use<ViewPayload>(dbName);

function emit(key: any, value: any) {
  _emit(key, value);
}

beforeEach(async () => {
  await resetTestDb(dbName);
});

afterEach(async () => {
  await testNano.db.destroy(dbName).catch(pass);
  jest.resetModules();
  jest.restoreAllMocks();
});

it("adds all datum views to an empty db", async () => {
  const datumView1 = {
    name: "datum_view",
    map: (doc: any) => {
      emit(doc._id, 1);
    },
    reduce: "_count",
  };
  const datumView2 = {
    name: "datum_another_view",
    map: (doc: any) => {
      emit(doc._rev, null);
    },
  };

  const mockAllDatumViews = [datumView1, datumView2];
  const getAllDatumViewsSpy = jest
    .spyOn(getAllDatumViews, "default")
    .mockReturnValue(mockAllDatumViews);

  const insertDatumViewsSpy = jest.spyOn(insertDatumViewModule, "default");

  await setupDatumViews({ db });

  await db.get("_design/datum_view");
  await db.get("_design/datum_another_view");

  expect(insertDatumViewsSpy).toHaveBeenCalledTimes(2);

  insertDatumViewsSpy.mockRestore();
  getAllDatumViewsSpy.mockRestore();
});
