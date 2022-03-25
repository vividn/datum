import {
  DataOnlyDocument,
  DatumDocument,
} from "../../documentControl/DatumDocument";
import { DateTime, Settings } from "luxon";
import { getOccurTime } from "../getOccurTime";

describe("getOccurTime", () => {
  it("gets the occurTime of a datum doc with the right offset", () => {
    const datumDoc: DatumDocument = {
      _id: "some_datum_document",
      _rev: "some_revison",
      data: { occurTime: "2022-03-06T07:00:00.000Z" },
      meta: { utcOffset: -8 },
    };
    const expectedLuxonDateTime = DateTime.local(2022, 3, 5, 23, {
      zone: "UTC-8",
    });
    const returnVal = getOccurTime(datumDoc);
    expect(expectedLuxonDateTime.equals(returnVal!)).toBeTruthy();
  });

  it("returns undefined for a datum doc without an occurTime", () => {
    const datumDoc: DatumDocument = {
      _id: "some_datum_document",
      _rev: "some_revison",
      data: { foo: "bar" },
      meta: { utcOffset: -8 },
    };

    const returnVal = getOccurTime(datumDoc);
    expect(returnVal).toBeUndefined();
  });

  it("formats a data only doc with the local offset", () => {
    Settings.defaultZone = "UTC-9";
    const dataDoc: DataOnlyDocument = {
      _id: "some_data_only_doc",
      _rev: "some_revision",
      occurTime: "2022-03-06T07:00:00.000Z",
    };
    const expectedLuxonDateTime = DateTime.local(2022, 3, 5, 22);
    const returnVal = getOccurTime(dataDoc);
    expect(expectedLuxonDateTime.equals(returnVal!)).toBeTruthy();
    Settings.defaultZone = "system";
  });

  it("returns undefined for a data only doc without an occurTime", () => {
    const dataDoc: DataOnlyDocument = {
      _id: "some_data_only_doc",
      _rev: "some_revision",
      foo: "bar",
    };
    const returnVal = getOccurTime(dataDoc);
    expect(returnVal).toBeUndefined();
  });
});