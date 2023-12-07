import { addIdAndMetadata } from "../addIdAndMetadata";
import { setNow } from "../../__test__/test-utils";
import { IdError } from "../../errors";
import { toDatumTime } from "../../time/timeUtils";

const nowUtc = "2023-09-05T11:35:00.000Z";
const nowDatumTime = toDatumTime(nowUtc);
describe("addIdAndMetadata", () => {
  beforeEach(() => {
    setNow("2023-09-05,11:35");
  });

  it("adds id and metadata in a sane and stable way 1", () => {
    expect(addIdAndMetadata({ abc: "ghi" }, {})).toEqual({
      _id: "ghi",
      data: {
        abc: "ghi",
      },
      meta: {
        createTime: nowDatumTime,
        humanId: expect.any(String),
        idStructure: "%abc%",
        modifyTime: nowDatumTime,
        random: expect.any(Number),
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
        random: expect.any(Number),
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
        { idPart: ["%abc"] },
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
        random: expect.any(Number),
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
        idStructure: "%field%:%occurTime%",
        createTime: nowDatumTime,
        humanId: expect.any(String),
        modifyTime: nowDatumTime,
        random: expect.any(Number),
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
        partition: "%foo",
        idPart: "%?humanId",
      },
    );
    expect(payload).toMatchObject({
      data: {
        field: "field",
        foo: "bar",
        occurTime: { utc: "2023-09-05T11:20:00.000Z", o: 0, tz: "UTC" },
      },
      meta: {
        idStructure: "%foo%:%?humanId%",
      },
    });
    const hid = payload.meta.humanId;
    expect(payload._id).toEqual(`bar:${hid}`);
  });

  it("adds id and metadata in a sane and stable way 7", () => {
    const payload = addIdAndMetadata(
      {
        foo: "bar",
        field: "field",
        occurTime: { utc: "2023-09-05T11:20:00.000Z" },
      },
      {
        partition: "%foo",
        idPart: ["%occurTime", "%?humanId"],
        idDelimiter: "!!!",
      },
    );
    expect(payload).toMatchObject({
      data: {
        field: "field",
        foo: "bar",
        occurTime: {
          utc: "2023-09-05T11:20:00.000Z",
        },
      },
      meta: {
        idStructure: "%foo%:%occurTime%!!!%?humanId%",
      },
    });
    const hid = payload.meta.humanId;
    expect(payload._id).toEqual(`bar:2023-09-05T11:20:00.000Z!!!${hid}`);
  });

  it("throws an error if the derived id is blank", () => {
    expect(() => addIdAndMetadata({}, {})).toThrow(IdError);
    expect(() => addIdAndMetadata({ foo: "bar" }, { idPart: [""] })).toThrow(
      IdError,
    );
  });
});
