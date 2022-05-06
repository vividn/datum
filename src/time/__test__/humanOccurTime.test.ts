import * as humanTime from "../humanTime";
import {
  DataOnlyDocument,
  DatumDocument,
} from "../../documentControl/DatumDocument";
import { humanOccurTime } from "../humanOccurTime";
import { DateTime, Settings } from "luxon";
import SpyInstance = jest.SpyInstance;

describe("humanOccurTime", () => {
  let humanTimeSpy: SpyInstance;

  beforeEach(() => {
    humanTimeSpy = jest.spyOn(humanTime, "humanTime");
  });

  it("formats a datum doc with the right offset", () => {
    const datumDoc: DatumDocument = {
      _id: "some_datum_document",
      _rev: "some_revison",
      data: { occurTime: "2022-03-06T07:00:00.000Z", occurUtcOffset: -8 },
      meta: {},
    };
    const expectedLuxonDateTime = DateTime.local(2022, 3, 5, 23, {
      zone: "UTC-8",
    });
    humanTimeSpy.mockReturnValue("formatted_human_time");
    const returnVal = humanOccurTime(datumDoc);
    expect(returnVal).toEqual("formatted_human_time");
    expect(
      expectedLuxonDateTime.equals(humanTimeSpy.mock.calls[0][0])
    ).toBeTruthy();
  });

  it("formats a data only doc with the local offset", () => {
    Settings.defaultZone = "UTC-9";
    const dataDoc: DataOnlyDocument = {
      _id: "some_data_only_doc",
      _rev: "some_revision",
      occurTime: "2022-03-06T07:00:00.000Z",
    };
    const expectedLuxonDateTime = DateTime.local(2022, 3, 5, 22);
    humanTimeSpy.mockReturnValue("formatted_human_time");
    const returnVal = humanOccurTime(dataDoc);
    expect(returnVal).toEqual("formatted_human_time");
    expect(
      expectedLuxonDateTime.equals(humanTimeSpy.mock.calls[0][0])
    ).toBeTruthy();
    Settings.defaultZone = "system";
  });
});
