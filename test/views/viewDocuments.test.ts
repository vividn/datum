import { it, describe, expect } from "@jest/globals";
import {
  DatumView,
  datumViewToViewPayload,
} from "../../src/views/viewDocument";
import emit from "../../src/views/emit";

describe("datumViewToViewPayload", () => {
  it("turns the name into a _design id", () => {
    const datumView: DatumView = {
      name: "the_name",
      map: (_doc) => {},
    };
    const viewPayload = datumViewToViewPayload(datumView);
    expect(viewPayload).toHaveProperty("_id", "_design/the_name");
  });

  it("has a 'default' view with stringified map doc if no reduce is given", () => {
    const mapFunc = (doc: any) => {
      emit(doc._id, null);
    };
    const datumView: DatumView = {
      name: "the_name",
      map: (_doc) => {},
    };
    const viewPayload = datumViewToViewPayload(datumView);
    expect(viewPayload).toHaveProperty("views.default");
    expect(viewPayload).toHaveProperty("views.default.map", mapFunc.toString());
  });
  
  it.todo("uses a 'default' view if just one reduce is given");
  it.todo("stringifies reduce if it is function");
  it.todo("keeps the reduce string if it is a special case");
  it.todo(
    "uses the names of the reduce functions as the views in the design document"
  );
  it.todo(
    "has a default view with just the map document if no reduce is named default"
  );
  it.todo;
  it.todo("adds an empty meta object");
});

describe("insertDatumView", () => {
  it.todo("turns a DatumView into a functioning view");
  it.todo("overwrites an existing view with the new function contents");
});

it.todo("adds all datum views to an empty db");
