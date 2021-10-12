import { afterEach, beforeEach, it } from "@jest/globals";
import timezone_mock from "timezone-mock";
import { DateTime, Settings } from "luxon";

beforeEach(() => {
    timezone_mock.register("UTC");
    const mockNow = DateTime.utc(2020, 5, 10, 15, 25, 30).toMillis();
    Settings.now = () => mockNow;
  });

afterEach(() => {
  timezone_mock.unregister();
  Settings.resetCaches();
});

it("handles absolute time strings", () => {
    expectTiming({ time: "3:15" }, "2020-05-10T03:15:00.000Z");
    expectTiming({ time: "15:15" }, "2020-05-10T15:15:00.000Z");
    expectTiming({ time: "3:15am" }, "2020-05-10T03:15:00.000Z");
    expectTiming({ time: "3:15pm" }, "2020-05-10T15:15:00.000Z");
    expectTiming({ time: "315" }, "2020-05-10T03:15:00.000Z");
    expectTiming({ time: "0315" }, "2020-05-10T03:15:00.000Z");
    expectTiming({ time: "2315" }, "2020-05-10T23:15:00.000Z");
    expectTiming({ time: "10" }, "2020-05-10T10:00:00.000Z");
    expectTiming({ time: "0" }, "2020-05-10T00:00:00.000Z");
    expectTiming({ time: "24" }, "2020-05-11T00:00:00.000Z");
    expectTiming({ time: "16:25:20.555" }, "2020-05-10T16:25:20.555Z");
    expectTiming({ time: "16:45:04" }, "2020-05-10T16:45:04.000Z");
    expectTiming({ time: "3:45:04pm" }, "2020-05-10T15:45:04.000Z");
    expectTiming({ time: "2010-05-20T10:00:00Z" }, "2010-05-20T10:00:00.000Z");
    expectTiming({ time: "yesterday at 10:17" }, "2020-05-09T10:17:00.000Z");
  });