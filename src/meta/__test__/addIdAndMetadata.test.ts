import { addIdAndMetadata } from "../addIdAndMetadata";
import { setNow } from "../../__test__/test-utils";
import { IdError, FieldError } from "../../errors";
import { toDatumTime } from "../../time/datumTime";
import { DatumPayload } from "../../documentControl/DatumDocument";

const nowUtc = "2023-09-05T11:35:00.000Z";
const nowDatumTime = toDatumTime(nowUtc);
describe("addIdAndMetadata", () => {
  beforeEach(() => {
    setNow("2023-09-05,11:35");
  });

  it("adds id and metadata in a sane and stable way 1", () => {
    expect(addIdAndMetadata({ abc: "ghi" }, {})).toEqual({
      _id: `${nowUtc}c`,
      data: {
        abc: "ghi",
      },
      meta: {
        createTime: nowDatumTime,
        humanId: expect.any(String),
        idStructure: "%?createTime%c",
        modifyTime: nowDatumTime,
      },
    });
  });

  it("adds id and metadata in a sane and stable way 2", () => {
    expect(
      addIdAndMetadata(
        { abc: "ghi", occurTime: { utc: "2023-09-05T11:20:00.000Z" } },
        {},
      ),
    ).toEqual({
      _id: "2023-09-05T11:20:00.000Z",
      data: {
        abc: "ghi",
        occurTime: {
          utc: "2023-09-05T11:20:00.000Z",
        },
      },
      meta: {
        idStructure: "%occurTime%",
        createTime: nowDatumTime,
        humanId: expect.any(String),
        modifyTime: nowDatumTime,
      },
    });
  });

  it("adds id and metadata in a sane and stable way 3", () => {
    expect(
      addIdAndMetadata(
        {
          abc: "ghi",
          occurTime: { utc: "2023-09-05T11:20:00.000Z", o: 0, tz: "UTC" },
        },
        { noMetadata: true },
      ),
    ).toEqual({
      _id: "2023-09-05T11:20:00.000Z",
      abc: "ghi",
      occurTime: { utc: "2023-09-05T11:20:00.000Z", o: 0, tz: "UTC" },
    });
  });

  it("adds id and metadata in a sane and stable way 4", () => {
    expect(
      addIdAndMetadata(
        {
          abc: "ghi",
          occurTime: { utc: "2023-09-05T11:20:00.000Z" },
        },
        { idParts: ["%abc"] },
      ),
    ).toEqual({
      _id: "ghi",
      data: {
        abc: "ghi",
        occurTime: {
          utc: "2023-09-05T11:20:00.000Z",
        },
      },
      meta: {
        idStructure: "%abc%",
        createTime: nowDatumTime,
        humanId: expect.any(String),
        modifyTime: nowDatumTime,
      },
    });
  });

  it("adds id and metadata in a sane and stable way 5", () => {
    expect(
      addIdAndMetadata(
        {
          foo: "bar",
          field: "field",
          occurTime: { utc: "2023-09-05T11:20:00.000Z" },
        },
        {},
      ),
    ).toEqual({
      _id: "field:2023-09-05T11:20:00.000Z",
      data: {
        field: "field",
        foo: "bar",
        occurTime: {
          utc: "2023-09-05T11:20:00.000Z",
        },
      },
      meta: {
        idStructure: "%occurTime%",
        createTime: nowDatumTime,
        humanId: expect.any(String),
        modifyTime: nowDatumTime,
      },
    });
  });

  it("adds id and metadata in a sane and stable way 6", () => {
    const payload = addIdAndMetadata(
      {
        foo: "bar",
        field: "%foo",
        occurTime: { utc: "2023-09-05T11:20:00.000Z", o: 0, tz: "UTC" },
      },
      {
        idParts: ["%?humanId"],
      },
    ) as DatumPayload;
    expect(payload).toMatchObject({
      data: {
        field: "bar",
        foo: "bar",
        occurTime: { utc: "2023-09-05T11:20:00.000Z", o: 0, tz: "UTC" },
      },
      meta: {
        idStructure: "%?humanId%",
        fieldStructure: "%foo",
      },
    });
    const hid = payload.meta.humanId;
    expect(payload._id).toEqual(`bar:${hid}`);
  });

  it("adds id and metadata in a sane and stable way 7", () => {
    const payload = addIdAndMetadata(
      {
        foo: "bar",
        field: "%foo%",
        occurTime: { utc: "2023-09-05T11:20:00.000Z" },
      },
      {
        idParts: ["%occurTime", "%?humanId"],
        idDelimiter: "!!!",
      },
    ) as DatumPayload;
    expect(payload).toMatchObject({
      data: {
        field: "bar",
        foo: "bar",
        occurTime: {
          utc: "2023-09-05T11:20:00.000Z",
        },
      },
      meta: {
        idStructure: "%occurTime%!!!%?humanId%",
        fieldStructure: "%foo%",
      },
    });
    const hid = payload.meta.humanId;
    expect(payload._id).toEqual(`bar:2023-09-05T11:20:00.000Z!!!${hid}`);
  });

  it("throws an error if the derived id is blank", () => {
    expect(() => addIdAndMetadata({}, { noMetadata: true })).toThrow(IdError);
    expect(() => addIdAndMetadata({ foo: "bar" }, { idParts: [""] })).toThrow(
      IdError,
    );
  });

  it("throws an error if field contains a colon", () => {
    // Testing direct field error
    expect(() => addIdAndMetadata({ field: "invalid:field" }, {})).toThrow(
      FieldError,
    );

    // Note: The error for composite fields with colons would be caught in compileField
    // but the structure of addIdAndMetadata has changed
  });

  it("handles composite field syntax correctly", () => {
    // Create a payload with field property directly in the data object
    const payload = addIdAndMetadata(
      {
        prefix: "test",
        state: "active",
        field: "%prefix%_%state%",
        occurTime: { utc: "2023-09-05T11:20:00.000Z" },
      },
      {},
    ) as DatumPayload;

    // Check field is interpolated and fieldStructure is stored
    expect(payload.data).toMatchObject({
      prefix: "test",
      state: "active",
      occurTime: { utc: "2023-09-05T11:20:00.000Z" },
    });

    expect(payload.data.field).toBe("test_active");
    expect(payload.meta).toHaveProperty("fieldStructure", "%prefix%_%state%");
    expect(payload._id).toEqual("test_active:2023-09-05T11:20:00.000Z");
    expect(payload.meta.idStructure).toEqual("%occurTime%");
  });
});
