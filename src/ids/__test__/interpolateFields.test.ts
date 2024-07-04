import { interpolateFields } from "../../utils/interpolateFields";
import { DatumData, DatumMetadata } from "../../documentControl/DatumDocument";

describe("interpolateFields", () => {
  it("interpolates %fields% and %?metaFields% of datum doc", async () => {
    const data: DatumData = {
      foo: "bar",
      another: 123,
      occurTime: { utc: "2022-05-01T22:52:00Z" },
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
      "raw_string_bar???abcde--2022-05-01T22:52:00Z",
    );
  });

  it("can interpolate the occurTime field as a humanTime", () => {
    const data: DatumData = {
      foo: "bar",
      another: 123,
      occurTime: {
        utc: "2022-05-01T22:52:00Z",
        o: -3,
        tz: "America/Sao_Paulo",
      },
    };
    const interpolated = interpolateFields({
      data,
      format: "%occurTime% %foo%",
      useHumanTimes: true,
    });

    expect(interpolated).toMatchInlineSnapshot(`"2022-05-01 19:52:00-3 bar"`);
  });
});
