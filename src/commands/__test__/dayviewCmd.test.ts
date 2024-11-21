import {
  mockedLogLifecycle,
  mockSpecs,
  testDbLifecycle,
} from "../../__test__/test-utils";
import { generateSampleMorning } from "../../__test__/generateSampleMorning";
import { setupCmd } from "../setupCmd";
import { dayviewCmd } from "../dayviewCmd";
import fs from "fs";

describe("dayview", () => {
  const dbName = "dayview_test";
  testDbLifecycle(dbName);
  mockedLogLifecycle();
  mockSpecs();

  beforeEach(async () => {
    setupCmd({});
  });

  it("consistently generates a sample morning svg", async () => {
    await generateSampleMorning("2024-11-20");
    await dayviewCmd({
      endDate: "2024-11-20",
      nDays: 1,
      width: 1000,
      dayHeight: 100,
      outputFile: "/tmp/sample_morning.svg",
    });
    const svg = fs.readFileSync("/tmp/sample_morning.svg", "utf8");
    expect(svg).toMatchSnapshot();
    // write svg to snapshot folder too for github viewing
    fs.writeFileSync(__dirname + "/__snapshots__/sample_morning.svg", svg);
  });
});
