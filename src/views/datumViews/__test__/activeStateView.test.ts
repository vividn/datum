import * as emit from "../../emit";
import { makeDoc } from "../../../__test__/test-utils";
import { activeStateView } from "../activeStateView";
import { DateTime } from "luxon";
import { DatumTime } from "../../../time/datumTime";

const occurTime: DatumTime = {
  utc: "2023-08-22T15:00:00.000Z",
};
const occurDateTime: DateTime = DateTime.fromISO(occurTime.utc);

describe("activeStateView", () => {
  let emitMock: jest.SpyInstance;
  beforeEach(() => {
    emitMock = jest.spyOn(emit, "_emit");
  });

  it("emits nothing if there is no field", () => {
    const doc = makeDoc({ occurTime, state: true });
    const doc2 = makeDoc({
      occurTime,
      state: true,
      dur: "PT5M",
    });
    const doc3 = makeDoc({
      occurTime,
      dur: "PT10M",
    });
    activeStateView.map(doc);
    activeStateView.map(doc2);
    activeStateView.map(doc3);
    expect(emitMock).toHaveBeenCalledTimes(0);
  });

  it("emits nothing if just occurTime and dur is null", () => {
    const doc = makeDoc({
      field: "foo",
      dur: null,
      occurTime,
    });
    activeStateView.map(doc);
    expect(emitMock).not.toHaveBeenCalled();
  });

  it("emits nothing if just occurTime", () => {
    const doc = makeDoc({
      field: "foo",
      occurTime,
    });
    activeStateView.map(doc);
    expect(emitMock).not.toHaveBeenCalled();
  });

  it("emits nothing if just dur and state with no occurTime", () => {
    const doc = makeDoc({ field: "foo", dur: "PT3M", state: true });
    activeStateView.map(doc);
    expect(emitMock).not.toHaveBeenCalled();
  });

  it("emits ([field, occurTime], state) when there is no dur", () => {
    const doc = makeDoc({
      field: "foo",
      occurTime,
      state: true,
    });
    activeStateView.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(1);
    expect(emitMock).toHaveBeenCalledWith(["foo", occurTime.utc], true);
  });

  it("can handle a string state", () => {
    const doc = makeDoc({
      field: "bar",
      occurTime,
      state: "active",
    });
    activeStateView.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(1);
    expect(emitMock).toHaveBeenCalledWith(["bar", occurTime.utc], "active");
  });

  it("emits entries for both state and lastState if dur is present", () => {
    const doc = makeDoc({
      field: "bar",
      dur: "PT3M",
      state: "state1",
      lastState: "state2",
      occurTime,
    });
    activeStateView.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(2);
    expect(emitMock).toHaveBeenCalledWith(
      ["bar", occurDateTime.minus({ minutes: 3 }).toUTC().toISO()], // state started 3 minutes ago
      "state1",
    );
    expect(emitMock).toHaveBeenCalledWith(
      ["bar", occurTime.utc], // and then lastState was restored now
      "state2",
    );
  });

  it("emits only entry for state and not last state if dur is not present", () => {
    const doc = makeDoc({
      field: "bar",
      state: "state1",
      lastState: "state2",
      occurTime,
    });
    activeStateView.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(1);
    expect(emitMock).toHaveBeenCalledWith(["bar", occurTime.utc], "state1");
  });

  it("assumes that lastState is true if state is false", () => {
    const doc = makeDoc({
      field: "bar",
      dur: "PT1H",
      state: false,
      occurTime,
    });
    activeStateView.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(2);
    expect(emitMock).toHaveBeenCalledWith(
      ["bar", occurDateTime.minus({ hours: 1 }).toUTC().toISO()],
      false,
    );
    expect(emitMock).toHaveBeenCalledWith(["bar", occurTime.utc], true);
  });

  it("assumes that lastState is false if state is not false", () => {
    const doc = makeDoc({
      field: "bar",
      dur: "PT1H30M",
      state: true,
      occurTime,
    });
    activeStateView.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(2);
    expect(emitMock).toHaveBeenCalledWith(
      ["bar", occurDateTime.minus({ hours: 1, minutes: 30 }).toUTC().toISO()],
      true,
    );
    expect(emitMock).toHaveBeenCalledWith(["bar", occurTime.utc], false);

    emitMock.mockReset();

    const doc2 = makeDoc({
      field: "foobar",
      dur: "P1DT2H30M30S",
      state: "stringState",
      occurTime,
    });
    activeStateView.map(doc2);
    expect(emitMock).toHaveBeenCalledTimes(2);
    expect(emitMock).toHaveBeenCalledWith(
      [
        "foobar",
        occurDateTime
          .minus({ days: 1, hours: 2, minutes: 30, seconds: 30 })
          .toUTC()
          .toISO(),
      ],
      "stringState",
    );
    expect(emitMock).toHaveBeenCalledWith(["foobar", occurTime.utc], false);
  });

  it("assumes state is true and lastState is false if occurTime and dur are present", () => {
    const doc = makeDoc({
      field: "bar",
      dur: "PT10M",
      occurTime,
    });
    activeStateView.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(2);
    expect(emitMock).toHaveBeenCalledWith(
      ["bar", occurDateTime.minus({ minutes: 10 }).toUTC().toISO()], // assumes state is true, so started dur minutes ago
      true,
    );
    expect(emitMock).toHaveBeenCalledWith(
      ["bar", occurTime.utc], // and then lastState (assumed to be false) is restored now
      false,
    );
  });

  it("emits a false block if dur is negative returning to state", () => {
    const doc = makeDoc({
      field: "bar",
      dur: "-PT10M",
      state: true,
      occurTime,
    });
    activeStateView.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(2);
    expect(emitMock).toHaveBeenCalledWith(
      ["bar", occurDateTime.minus({ minutes: 10 }).toUTC().toISO()], // assumes state is false
      false,
    );
    expect(emitMock).toHaveBeenCalledWith(["bar", occurTime.utc], true);

    emitMock.mockReset();

    const doc2 = makeDoc({
      field: "foobar",
      dur: "-P1DT2H30M30S",
      state: "stringState",
      occurTime,
    });
    activeStateView.map(doc2);
    expect(emitMock).toHaveBeenCalledTimes(2);
    expect(emitMock).toHaveBeenCalledWith(
      [
        "foobar",
        occurDateTime
          .minus({ days: 1, hours: 2, minutes: 30, seconds: 30 })
          .toUTC()
          .toISO(),
      ],
      false,
    );
    expect(emitMock).toHaveBeenCalledWith(
      ["foobar", occurTime.utc],
      "stringState",
    );
  });

  it("emits nothing if duration is null or 0", () => {
    const doc = makeDoc({
      field: "bar",
      dur: null,
      state: true,
      occurTime,
    });
    activeStateView.map(doc);
    expect(emitMock).not.toHaveBeenCalled();

    emitMock.mockReset();

    const doc2 = makeDoc({
      field: "foobar",
      dur: "PT0S",
      state: "stringState",
      occurTime,
    });
    activeStateView.map(doc2);
    expect(emitMock).not.toHaveBeenCalled();
  });

  it("changes the active state to false if lastState is null and duration is nonnull", () => {
    const doc = makeDoc({
      field: "bar",
      dur: "PT10M",
      state: true,
      lastState: null,
      occurTime,
    });
    activeStateView.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(2);
    expect(emitMock).toHaveBeenCalledWith(
      ["bar", occurDateTime.minus({ minutes: 10 }).toUTC().toISO()], // assumes state is false
      true,
    );
    expect(emitMock).toHaveBeenCalledWith(["bar", occurTime.utc], false);
  });

  it("changes the active state to false if lastState is null and duration is null", () => {
    const doc = makeDoc({
      field: "bar",
      lastState: null,
      dur: null,
      occurTime,
    });
    activeStateView.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(1);
    expect(emitMock).toHaveBeenCalledWith(["bar", occurTime.utc], false);
  });
});
