import { interpolateFields } from "../interpolateFields";
import { DatumData, DatumMetadata } from "../../documentControl/DatumDocument";
import * as humanTime from "../../time/humanTime";

describe("interpolateFields", () => {
  it("interpolates %fields% and %?metaFields% of datum doc", async () => {
    const data: DatumData = {
      foo: "bar",
      another: 123,
      occurTime: "2022-05-01T22:52:00Z",
    };
    const meta: DatumMetadata = {
      humanId: "abcde",
    };

    const interpolated = interpolateFields({
      data,
      meta,
      format: "raw_string_%foo%???%?humanId%--%occurTime%",
    });
    expect(interpolated).toEqual(
      "raw_string_bar???abcde--2022-05-01T22:52:00Z"
    );
  });

  it("can interpolate the occurTime field as a humanTime", () => {
    const data: DatumData = {
      foo: "bar",
      another: 123,
      occurTime: "2022-05-01T22:52:00Z",
    };
    jest.spyOn(humanTime, "humanTimeFromISO").mockReturnValue("19:52");
    const interpolated = interpolateFields({
      data,
      format: "%occurTime% %foo%",
      useHumanTimes: true,
    });

    expect(interpolated).toEqual("19:52 bar");
  });
});
