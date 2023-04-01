import {
  asViewDb,
  DatumView,
  datumViewToViewPayload,
  StringifiedDatumView,
} from "../DatumView";
import { _emit } from "../emit";
import { testDbLifecycle } from "../../test-utils";
import { insertDatumView } from "../insertDatumView";
import * as addDoc from "../../documentControl/addDoc";
import * as overwriteDoc from "../../documentControl/overwriteDoc";

function emit(key: unknown, value: unknown) {
  _emit(key, value);
}

const genericMapFunction = (doc: any) => {
  emit(doc._id, null);
};

const genericMapStr = `doc => {
  emit(doc._id, null);
}`;

const genericReduceFunction = (
  _keys: any[],
  _values: any[],
  _rereduce: boolean
) => {
  return 0;
};
const genericReduceStr = `(_keys, _values, _rereduce) => {
  return 0;
}`;

test("string representations match toString return value", () => {
  expect(genericMapFunction.toString()).toEqual(genericMapStr);
  expect(genericReduceFunction.toString()).toEqual(genericReduceStr);
});

describe("datumViewToViewPayload", () => {
  it("turns the name into a _design id", () => {
    const datumView: DatumView = {
      name: "the_name",
      emit,
      map: (_doc) => {
        return;
      },
    };
    const viewPayload = datumViewToViewPayload(datumView);
    expect(viewPayload).toHaveProperty("_id", "_design/the_name");
  });

  it("has a 'default' view with stringified map doc if no reduce is given", () => {
    const datumView: DatumView = {
      name: "has_a_default_view",
      emit,
      map: genericMapFunction,
    };
    const viewPayload = datumViewToViewPayload(datumView);
    expect(viewPayload).toHaveProperty("views.default");
    expect(viewPayload).toHaveProperty("views.default.map", genericMapStr);
  });

  it("can create a 'default' view with stringified map and reduce", () => {
    const datumView: DatumView = {
      name: "with_reduce_still_has_default",
      emit,
      map: genericMapFunction,
      reduce: {
        default: "_count",
      },
    };
    const viewPayload = datumViewToViewPayload(datumView);
    expect(viewPayload).toHaveProperty("views.default");
    expect(viewPayload).toHaveProperty("views.default.map", genericMapStr);
  });

  it("stringifies reduce if it is function", () => {
    const datumView: DatumView = {
      name: "stringified_reduce",
      emit,
      map: genericMapFunction,
      reduce: {
        default: genericReduceFunction,
      },
    };
    const viewPayload = datumViewToViewPayload(datumView);
    expect(viewPayload).toHaveProperty(
      "views.default.reduce",
      genericReduceStr
    );
  });

  it("keeps the reduce string if it is a special case", () => {
    const viewPayload = datumViewToViewPayload({
      name: "special_reduce_string",
      emit,
      map: genericMapFunction,
      reduce: {
        default: "_count",
      },
    });
    expect(viewPayload).toHaveProperty("views.default.reduce", "_count");
  });

  it("uses the names of multiple reduce functions as the views in the design document, all with the same map", () => {
    const viewPayload = datumViewToViewPayload({
      name: "multiple_reduce",
      emit,
      map: genericMapFunction,
      reduce: {
        count: "_count",
        default: genericReduceFunction,
        anotherView: genericReduceFunction,
      },
    });
    const expectedViews = {
      views: {
        anotherView: {
          map: genericMapStr,
          reduce: genericReduceStr,
        },
        count: {
          map: genericMapStr,
          reduce: "_count",
        },
        default: {
          map: genericMapStr,
          reduce: genericReduceStr,
        },
      },
    };
    expect(viewPayload).toMatchObject(expectedViews);
  });

  it("has a default view with just the map document if no reduce is named default", () => {
    const viewPayload = datumViewToViewPayload({
      name: "multiple_reduce_no_default",
      emit,
      map: genericMapFunction,
      reduce: {
        count: "_count",
        anotherView: genericReduceFunction,
      },
    });
    expect(viewPayload).toHaveProperty("views.default.map", genericMapStr);
    expect(viewPayload).not.toHaveProperty("views.default.reduce");
  });

  it("adds an empty meta object", () => {
    expect(
      datumViewToViewPayload({
        name: "no_reduce",
        emit,
        map: genericMapFunction,
      })
    ).toHaveProperty("meta", {});
    expect(
      datumViewToViewPayload({
        name: "one_reduce",
        emit,
        map: genericMapFunction,
        reduce: {
          default: "_count",
        },
      })
    ).toHaveProperty("meta", {});
    expect(
      datumViewToViewPayload({
        name: "several_reduce",
        emit,
        map: genericMapFunction,
        reduce: {
          one: genericReduceFunction,
          two: genericReduceFunction,
        },
      })
    ).toHaveProperty("meta", {});
  });

  it("can also accept prestringified versions of map and reduce", () => {
    const datumView = datumViewToViewPayload({
      name: "stringy_input",
      emit,
      map: genericMapStr,
      reduce: {
        one: "_sum",
        two: genericReduceStr,
      },
    });
    expect(datumView).toMatchObject({
      _id: "_design/stringy_input",
      views: {
        one: { map: genericMapStr, reduce: "_sum" },
        two: { map: genericMapStr, reduce: genericReduceStr },
        default: { map: genericMapStr },
      },
      meta: {},
    });
  });
});

describe("insertDatumView", () => {
  const dbName = "insert_datum_view_test";
  const db = testDbLifecycle(dbName);
  const viewDb = asViewDb(db);

  it("turns a DatumView into a functioning view", async () => {
    const summerAB: DatumView = {
      name: "summer",
      emit,
      map: (doc: any) => {
        if (doc.a) {
          emit("a", doc.a);
        }
        if (doc.b) {
          emit("b", doc.b);
        }
      },
      reduce: {
        default: "_sum",
      },
    };

    await db.put({ _id: "doc1", a: 3, b: 4 });
    await db.put({ _id: "doc2", a: 6 });

    await insertDatumView({ db: viewDb, datumView: summerAB });

    const total = await db.query("summer/default");
    expect(total.rows[0].value).toBe(13);

    const grouped = await db.query("summer/default", { group: true });
    expect(grouped.rows).toEqual([
      { key: "a", value: 9 },
      { key: "b", value: 4 },
    ]);

    const unreduced = await db.query("summer/default", { reduce: false });
    expect(unreduced.total_rows).toEqual(3);
  });

  it("turns a StringifiedDatumView into a functioning view", async () => {
    const summerAB: StringifiedDatumView = {
      name: "summer",
      map: `(doc) => {
        if (doc.a) {
          emit("a", doc.a);
        }
        if (doc.b) {
          emit("b", doc.b);
        }
      }`,
      reduce: {
        default: "_sum",
      },
    };

    await db.put({ _id: "doc1", a: 3, b: 4 });
    await db.put({ _id: "doc2", a: 6 });

    await insertDatumView({ db: viewDb, datumView: summerAB });

    const total = await db.query("summer/default");
    expect(total.rows[0].value).toBe(13);

    const grouped = await db.query("summer/default", { group: true });
    expect(grouped.rows).toEqual([
      { key: "a", value: 9 },
      { key: "b", value: 4 },
    ]);

    const unreduced = await db.query("summer/default", { reduce: false });
    expect(unreduced.total_rows).toEqual(3);
  });

  it("returns the new viewDoc", async () => {
    const datumView1: DatumView = {
      name: "datum_view",
      emit,
      map: genericMapFunction,
      reduce: {
        default: "_count",
      },
    };
    const newDesignDoc = await insertDatumView({
      db: viewDb,
      datumView: datumView1,
    });
    const dbDoc = await db.get("_design/datum_view");
    expect(newDesignDoc).toEqual(dbDoc);
  });

  it("overwrites an existing view if DatumView has same name but different contents", async () => {
    const datumView1: DatumView = {
      name: "datum_view",
      emit,
      map: genericMapFunction,
      reduce: {
        default: "_count",
      },
    };
    await insertDatumView({ db: viewDb, datumView: datumView1 });
    const designDoc1 = await viewDb.get("_design/datum_view");
    expect(designDoc1.views["default"].reduce).toEqual("_count");

    const datumView2: DatumView = {
      name: "datum_view",
      emit,
      map: genericMapFunction,
      reduce: {
        default: "_stats",
      },
    };
    const returnedDoc = await insertDatumView({
      db: viewDb,
      datumView: datumView2,
    });
    const designDoc2 = await viewDb.get("_design/datum_view");
    expect(returnedDoc).toEqual(designDoc2);

    expect(designDoc2.views["default"].reduce).toEqual("_stats");
    expect(designDoc1._rev).not.toEqual(designDoc2._rev);
  });

  it("does not overwrite if view is identical", async () => {
    const datumView: DatumView = {
      name: "datum_view",
      emit,
      map: genericMapFunction,
      reduce: {
        default: "_count",
      },
    };
    await insertDatumView({ db: viewDb, datumView: datumView });
    const designDoc1 = await viewDb.get("_design/datum_view");

    await insertDatumView({ db: viewDb, datumView: datumView });

    const designDoc2 = await viewDb.get("_design/datum_view");
    expect(designDoc1._rev).toEqual(designDoc2._rev);
  });

  it("calls addDoc", async () => {
    const addDocSpy = jest.spyOn(addDoc, "addDoc");
    const datumView: DatumView = {
      name: "datum_view",
      emit,
      map: genericMapFunction,
      reduce: {
        default: "_count",
      },
    };
    await insertDatumView({ db: viewDb, datumView: datumView });

    expect(addDocSpy).toHaveBeenCalledTimes(1);
  });

  it("calls overwriteDoc when overwriting", async () => {
    const overwriteDocSpy = jest.spyOn(overwriteDoc, "overwriteDoc");
    const datumView1: DatumView = {
      name: "datum_view",
      emit,
      map: genericMapFunction,
      reduce: {
        default: "_count",
      },
    };
    await insertDatumView({ db: viewDb, datumView: datumView1 });

    const datumView2: DatumView = {
      name: "datum_view",
      emit,
      map: genericMapFunction,
      reduce: {
        default: "_stats",
      },
    };
    await insertDatumView({ db: viewDb, datumView: datumView2 });

    expect(overwriteDocSpy).toHaveBeenCalledTimes(1);
  });
});
