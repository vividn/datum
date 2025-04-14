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
        field: "field",
        occurTime: { utc: "2023-09-05T11:20:00.000Z", o: 0, tz: "UTC" },
      },
      {
        idParts: ["%?humanId"],
      },
    ) as DatumPayload;
    expect(payload).toMatchObject({
      data: {
        field: "field",
        foo: "bar",
        occurTime: { utc: "2023-09-05T11:20:00.000Z", o: 0, tz: "UTC" },
      },
      meta: {
        idStructure: "%?humanId%",
      },
    });
    const hid = payload.meta.humanId;
    expect(payload._id).toEqual(`field:${hid}`);
  });

  it("adds id and metadata in a sane and stable way 7", () => {
    const payload = addIdAndMetadata(
      {
        foo: "bar",
        field: "field",
        occurTime: { utc: "2023-09-05T11:20:00.000Z" },
      },
      {
        idParts: ["%occurTime", "%?humanId"],
        idDelimiter: "!!!",
      },
    ) as DatumPayload;
    expect(payload).toMatchObject({
      data: {
        field: "field",
        foo: "bar",
        occurTime: {
          utc: "2023-09-05T11:20:00.000Z",
        },
      },
      meta: {
        idStructure: "%occurTime%!!!%?humanId%",
      },
    });
    const hid = payload.meta.humanId;
    expect(payload._id).toEqual(`field:2023-09-05T11:20:00.000Z!!!${hid}`);
  });

  it("throws an error if the derived id is blank", () => {
    expect(() => addIdAndMetadata({}, { noMetadata: true })).toThrow(IdError);
    expect(() => addIdAndMetadata({ foo: "bar" }, { idParts: [""] })).toThrow(
      IdError,
    );
  });
  
  it("throws an error if field contains a colon", () => {
    expect(() => 
      addIdAndMetadata({ field: "invalid:field" }, {})
    ).toThrow(FieldError);
    
    expect(() => 
      addIdAndMetadata({}, { field: "invalid:field" })
    ).toThrow(FieldError);
    
    // Test composite field that would generate a field with a colon
    expect(() =>
      addIdAndMetadata(
        { part1: "first", part2: "second" },
        { field: "%part1%:%part2%" }
      )
    ).toThrow(FieldError);
  });
  
  it("handles composite field syntax correctly", () => {
    const payload = addIdAndMetadata(
      {
        prefix: "test",
        state: "active",
        occurTime: { utc: "2023-09-05T11:20:00.000Z" },
      },
      {
        field: "%prefix%_%state%",
      },
    ) as DatumPayload;
    
    expect(payload.data).toMatchObject({
      prefix: "test",
      state: "active",
      field: "test_active",
      occurTime: { utc: "2023-09-05T11:20:00.000Z" },
    });
    
    expect(payload._id).toEqual("test_active:2023-09-05T11:20:00.000Z");
    expect(payload.meta.idStructure).toEqual("%occurTime%");
  });
});
