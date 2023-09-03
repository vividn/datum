import * as emit from "../../emit";
import { makeDoc } from "../../../test-utils";
import { activeStateView } from "../activeStateView";

describe("activeStateView", () => {
  let emitMock: any;
  beforeEach(() => {
    emitMock = jest.spyOn(emit, "_emit");
  });

  it("emits nothing if there is no field", () => {
    const doc = makeDoc({ occurTime: "2023-08-22T15:00:00.000Z", state: true });
    const doc2 = makeDoc({
      occurTime: "2023-08-22T15:00:00.000Z",
      state: true,
      dur: "PT5M",
    });
    const doc3 = makeDoc({
      occurTime: "2023-08-22T15:00:00.000Z",
      dur: "PT10M",
    });
    activeStateView.map(doc);
    activeStateView.map(doc2);
    activeStateView.map(doc3);
    expect(emitMock).toHaveBeenCalledTimes(0);
  });

  it("emits nothing if just occurTime with no dur or state", () => {
    const doc = makeDoc({
      field: "foo",
      occurTime: "2023-08-22T15:00:00.000Z",
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
      occurTime: "2023-08-22T15:00:00.000Z",
      state: true,
    });
    activeStateView.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(1);
    expect(emitMock).toHaveBeenCalledWith(
      ["foo", "2023-08-22T15:00:00.000Z"],
      true
    );
  });

  it("can handle a string state", () => {
    const doc = makeDoc({
      field: "bar",
      occurTime: "2023-08-22T15:00:00.000Z",
      state: "active",
    });
    activeStateView.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(1);
    expect(emitMock).toHaveBeenCalledWith(
      ["bar", "2023-08-22T15:00:00.000Z"],
      "active"
    );
  });

  it("emits entries for both state and lastState if dur is present", () => {
    const doc = makeDoc({
      field: "bar",
      dur: "PT3M",
      state: "state1",
      lastState: "state2",
      occurTime: "2023-08-22T15:00:00.000Z",
    });
    activeStateView.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(2);
    expect(emitMock).toHaveBeenCalledWith(
      ["bar", "2023-08-22T14:57:00.000Z"], // state started 3 minutes ago
      "state1"
    );
    expect(emitMock).toHaveBeenCalledWith(
      ["bar", "2023-08-22T15:00:00.000Z"], // and then lastState was restored now
      "state2"
    );
  });

  it("emits only entry for state and not last state if dur is not present", () => {
    const doc = makeDoc({
      field: "bar",
      state: "state1",
      lastState: "state2",
      occurTime: "2023-08-22T15:00:00.000Z",
    });
    activeStateView.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(1);
    expect(emitMock).toHaveBeenCalledWith(
      ["bar", "2023-08-22T15:00:00.000Z"],
      "state1"
    );
  });

  it("assumes that lastState is true if state is false", () => {
    const doc = makeDoc({
      field: "bar",
      dur: "PT1H",
      state: false,
      occurTime: "2023-08-22T15:00:00.000Z",
    });
    activeStateView.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(2);
    expect(emitMock).toHaveBeenCalledWith(
      ["bar", "2023-08-22T14:00:00.000Z"],
      false
    );
    expect(emitMock).toHaveBeenCalledWith(
      ["bar", "2023-08-22T15:00:00.000Z"],
      true
    );
  });

  it("assumes that lastState is false if state is not false", () => {
    const doc = makeDoc({
      field: "bar",
      dur: "PT1H30M",
      state: true,
      occurTime: "2023-08-22T15:00:00.000Z",
    });
    activeStateView.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(2);
    expect(emitMock).toHaveBeenCalledWith(
      ["bar", "2023-08-22T13:30:00.000Z"],
      true
    );
    expect(emitMock).toHaveBeenCalledWith(
      ["bar", "2023-08-22T15:00:00.000Z"],
      false
    );

    emitMock.mockReset();

    const doc2 = makeDoc({
      field: "foobar",
      dur: "P1DT2H30M30S",
      state: "stringState",
      occurTime: "2023-08-22T15:00:00.000Z",
    });
    activeStateView.map(doc2);
    expect(emitMock).toHaveBeenCalledTimes(2);
    expect(emitMock).toHaveBeenCalledWith(
      ["foobar", "2023-08-21T12:29:30.000Z"],
      "stringState"
    );
    expect(emitMock).toHaveBeenCalledWith(
      ["foobar", "2023-08-22T15:00:00.000Z"],
      false
    );
  });

  it("assumes state is true and lastState is false if occurTime and dur are present", () => {
    const doc = makeDoc({
      field: "bar",
      dur: "PT10M",
      occurTime: "2023-08-22T15:00:00.000Z",
    });
    activeStateView.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(2);
    expect(emitMock).toHaveBeenCalledWith(
      ["bar", "2023-08-22T14:50:00.000Z"], // assumes state is true, so started dur minutes ago
      true
    );
    expect(emitMock).toHaveBeenCalledWith(
      ["bar", "2023-08-22T15:00:00.000Z"], // and then lastState (assumed to be false) is restored now
      false
    );
  });

  it("emits a false block if dur is negative returning to state", () => {
    const doc = makeDoc({
      field: "bar",
      dur: "-PT10M",
      state: true,
      occurTime: "2023-08-22T15:00:00.000Z",
    });
    activeStateView.map(doc);
    expect(emitMock).toHaveBeenCalledTimes(2);
    expect(emitMock).toHaveBeenCalledWith(
      ["bar", "2023-08-22T14:50:00.000Z"],
      false
    );
    expect(emitMock).toHaveBeenCalledWith(
      ["bar", "2023-08-22T15:00:00.000Z"],
      true
    );

    emitMock.mockReset();

    const doc2 = makeDoc({
      field: "foobar",
      dur: "-P1DT2H30M30S",
      state: "stringState",
      occurTime: "2023-08-22T15:00:00.000Z",
    });
    activeStateView.map(doc2);
    expect(emitMock).toHaveBeenCalledTimes(2);
    expect(emitMock).toHaveBeenCalledWith(
      ["foobar", "2023-08-21T12:29:30.000Z"],
      false
    );
    expect(emitMock).toHaveBeenCalledWith(
      ["foobar", "2023-08-22T15:00:00.000Z"],
      "stringState"
    );
  });
});
